use std::sync::Arc;

use serde_json::{Value, json};
use skwd_wall_core::WallState;
use wall_proto::ev;

use crate::backend::events::EventPublisher;
use crate::backend::history::{ApplySource, HistoryRepository};
use crate::domain::history::HistoryEntry;
use crate::infrastructure::stats::Stats;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MediaKind {
    Static,
    Video,
    WallpaperEngine,
}

impl MediaKind {
    pub(super) fn wire(self) -> &'static str {
        match self {
            Self::Static => wall_proto::kind::STATIC,
            Self::Video => wall_proto::kind::VIDEO,
            Self::WallpaperEngine => wall_proto::kind::WE,
        }
    }
}

#[derive(Clone)]
pub(super) struct ApplyDecision {
    pub(super) generation: u64,
    pub(super) media: MediaKind,
    pub(super) path: String,
    pub(super) we_id: String,
    pub(super) output: String,
    pub(super) committed_outputs: Vec<String>,
    pub(super) mute: bool,
    pub(super) volume: u32,
    pub(super) source: ApplySource,
    pub(super) notify: bool,
    pub(super) random: bool,
    pub(super) library_key: String,
    pub(super) prior_entry: Option<HistoryEntry>,
}

pub(super) struct ExecutionReceipt {
    decision: ApplyDecision,
    theme_source: Option<String>,
    persisted_thumb: String,
}

impl ExecutionReceipt {
    pub(super) fn new(
        decision: ApplyDecision,
        theme_source: Option<String>,
        persisted_thumb: String,
    ) -> Self {
        Self { decision, theme_source, persisted_thumb }
    }

    /// Renderer success alone is not publication authority: a deferred apply
    /// may have been superseded while its renderer was reaching readiness.
    pub(super) fn commit(self, state: &WallState) -> anyhow::Result<CommittedApply> {
        authorize_commit(self.decision.generation, state.apply().generation())?;
        Ok(CommittedApply(self))
    }
}

fn authorize_commit(expected: u64, current: u64) -> anyhow::Result<()> {
    if current != expected {
        return Err(anyhow::Error::new(super::SupersededApply));
    }
    Ok(())
}

pub(super) struct CommittedApply(ExecutionReceipt);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PostCommitStep {
    Theme,
    Stats,
    AppliedEvent,
    Persistence,
    PostProcess,
    Overview,
    LockScreen,
    ApplyCount,
    Workspace,
    Notify,
    RestorePolicy,
    History,
}

impl CommittedApply {
    fn drive_publication(mut effect: impl FnMut(PostCommitStep)) {
        for step in [
            PostCommitStep::Theme,
            PostCommitStep::Stats,
            PostCommitStep::AppliedEvent,
            PostCommitStep::Persistence,
            PostCommitStep::PostProcess,
            PostCommitStep::Overview,
            PostCommitStep::LockScreen,
            PostCommitStep::ApplyCount,
            PostCommitStep::Workspace,
            PostCommitStep::Notify,
            PostCommitStep::RestorePolicy,
            PostCommitStep::History,
        ] {
            effect(step);
        }
    }

    /// Consuming the committed receipt makes publication exactly-once for one
    /// execution attempt. A retry receives a fresh generation and receipt.
    pub(super) fn publish(
        self,
        state: &Arc<WallState>,
        history: &dyn HistoryRepository,
        publisher: &dyn EventPublisher,
        stats: &Stats,
    ) -> Value {
        let receipt = self.0;
        let decision = &receipt.decision;
        crate::composition::history::note_apply_source(decision.source);
        let config = state.config().clone();
        Self::drive_publication(|step| match step {
            PostCommitStep::Theme => {
                if let Some(theme_source) = receipt.theme_source.as_deref() {
                    state.theme().set_source(theme_source);
                    crate::infrastructure::theme_worker::theme_apply_after_async(
                        theme_source,
                        super::theme_delay(),
                    );
                }
            }
            PostCommitStep::Stats => {
                for _ in &decision.committed_outputs {
                    stats.applied(decision.media.wire(), applied_identity(decision));
                }
            }
            PostCommitStep::AppliedEvent => {
                for output in &decision.committed_outputs {
                    publisher.publish(ev::APPLIED, applied_event(decision, output));
                }
            }
            PostCommitStep::Persistence => crate::infrastructure::persistence::persist_last(
                decision.media.wire(),
                &decision.path,
                &decision.we_id,
                decision.mute,
                decision.volume,
                &receipt.persisted_thumb,
            ),
            PostCommitStep::PostProcess => skwd_wall_core::postprocess::run(
                &config,
                decision.media.wire(),
                applied_identity(decision),
                &receipt.persisted_thumb,
                false,
            ),
            PostCommitStep::Overview => {
                crate::infrastructure::overview_backdrop::on_apply(&config);
            }
            PostCommitStep::LockScreen => {
                crate::infrastructure::lock_screen::request_follow_sync(state);
            }
            PostCommitStep::ApplyCount if !decision.library_key.is_empty() => {
                for _ in &decision.committed_outputs {
                    if let Err(error) = state.with_db(|connection| {
                        skwd_wall_core::db::bump_apply_count(connection, &decision.library_key)
                    }) {
                        log::warn!("could not record apply for {}: {error}", decision.library_key);
                    }
                }
            }
            PostCommitStep::Workspace
                if !matches!(decision.source, ApplySource::Workspace | ApplySource::Restore) =>
            {
                for output in &decision.committed_outputs {
                    crate::infrastructure::workspaces::note_external_apply(
                        state,
                        output,
                        decision.media.wire(),
                        &decision.path,
                        &decision.we_id,
                        decision.mute,
                        decision.volume,
                    );
                }
            }
            PostCommitStep::Notify if decision.notify => {
                skwd_wall_core::matugen::notify_change(&state.config());
            }
            PostCommitStep::RestorePolicy => {
                for output in &decision.committed_outputs {
                    if decision.source.updates_restore_policy() {
                        crate::infrastructure::restore_policy::record_apply(
                            output,
                            decision.media.wire(),
                            &decision.path,
                            &decision.we_id,
                        );
                    } else {
                        crate::infrastructure::restore_policy::record_restored(
                            output,
                            decision.media.wire(),
                            &decision.path,
                            &decision.we_id,
                        );
                    }
                }
            }
            PostCommitStep::History if decision.source.records() => {
                for output in &decision.committed_outputs {
                    crate::composition::history::record_history(
                        state,
                        history,
                        output,
                        decision.media.wire(),
                        &decision.path,
                        &decision.we_id,
                        decision.mute,
                        decision.volume,
                        decision.prior_entry.as_ref(),
                    );
                }
            }
            PostCommitStep::ApplyCount
            | PostCommitStep::Workspace
            | PostCommitStep::Notify
            | PostCommitStep::History => {}
        });
        json!({"applied": applied_identity(decision)})
    }
}

fn applied_identity(decision: &ApplyDecision) -> &str {
    if decision.media == MediaKind::WallpaperEngine { &decision.we_id } else { &decision.path }
}

fn applied_event(decision: &ApplyDecision, output: &str) -> Value {
    if decision.media == MediaKind::WallpaperEngine {
        json!({"key": decision.library_key, "we_id": decision.we_id, "type": decision.media.wire(), "random": decision.random, "output": output})
    } else {
        json!({"key": decision.library_key, "path": decision.path, "type": decision.media.wire(), "random": decision.random, "output": output})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_driver_executes_every_effect_exactly_once_in_order() {
        let expected = [
            PostCommitStep::Theme,
            PostCommitStep::Stats,
            PostCommitStep::AppliedEvent,
            PostCommitStep::Persistence,
            PostCommitStep::PostProcess,
            PostCommitStep::Overview,
            PostCommitStep::LockScreen,
            PostCommitStep::ApplyCount,
            PostCommitStep::Workspace,
            PostCommitStep::Notify,
            PostCommitStep::RestorePolicy,
            PostCommitStep::History,
        ];
        for media in [MediaKind::Static, MediaKind::Video, MediaKind::WallpaperEngine] {
            for output in ["*", "DP-1"] {
                let mut observed = Vec::new();
                CommittedApply::drive_publication(|step| observed.push(step));
                assert_eq!(observed, expected, "{media:?} {output}");
                for effect in expected {
                    assert_eq!(observed.iter().filter(|step| **step == effect).count(), 1);
                }
            }
        }
    }

    #[test]
    fn superseded_execution_has_no_commit_authority_for_any_media_or_scope() {
        for media in [MediaKind::Static, MediaKind::Video, MediaKind::WallpaperEngine] {
            for output in ["*", "DP-1"] {
                assert!(authorize_commit(7, 8).is_err(), "{media:?} {output}");
                assert!(authorize_commit(7, 6).is_err(), "{media:?} {output}");
                assert!(authorize_commit(7, 7).is_ok(), "{media:?} {output}");
            }
        }
    }

    #[test]
    fn retry_uses_a_fresh_commit_generation() {
        assert!(authorize_commit(41, 42).is_err(), "failed attempt cannot publish on retry");
        assert!(authorize_commit(42, 42).is_ok(), "retry owns the current publication slot");
    }
}
