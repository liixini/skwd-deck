use std::time::Duration;

use crate::backend::events::EventPublisher;
use crate::composition::context::Ctx;

const FILTER: &str = "type='signal',interface='org.kde.ActivityManager.Activities'";
const SIGNALS: &[&str] = &[
    "member=CurrentActivityChanged",
    "member=ActivityAdded",
    "member=ActivityRemoved",
    "member=ActivityNameChanged",
    "member=ActivityStateChanged",
];
const RECONNECT: Duration = Duration::from_secs(2);

fn changed(line: &str) -> bool {
    SIGNALS.iter().any(|signal| line.contains(signal))
}

pub(crate) fn start(ctx: Ctx) {
    if !skwd_wall_core::plasma::available() {
        return;
    }
    std::thread::spawn(move || {
        loop {
            let result = super::proc::monitor_signals(&[FILTER], |line| {
                if !changed(line) {
                    return;
                }
                {
                    let _apply = ctx.state.apply().lock();
                    if let Err(error) = skwd_wall_core::plasma::activities::sync_current(&ctx.state)
                    {
                        log::warn!("activity: refresh failed: {error:#}");
                    }
                }
                ctx.events.publish(wall_proto::ev::OUTPUTS_CHANGED, serde_json::json!({}));
            });
            if let Err(error) = result {
                log::warn!("activity: monitor failed: {error:#}");
            }
            std::thread::sleep(RECONNECT);
        }
    });
}

#[cfg(test)]
mod tests;
