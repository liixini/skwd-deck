#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ApplySource {
    User,
    UserOverride,
    Random,
    Rotation,
    Playlist,
    Schedule,
    Workspace,
    Restore,
    Hotplug,
    Replay,
}

impl ApplySource {
    pub(crate) fn broadcast_random(self) -> bool {
        matches!(self, Self::Rotation | Self::Random)
    }

    pub(crate) fn records(self) -> bool {
        matches!(self, Self::User | Self::UserOverride | Self::Random)
    }

    pub(crate) fn respects_output_locks(self) -> bool {
        !matches!(self, Self::UserOverride | Self::Restore | Self::Hotplug)
    }

    pub(crate) fn updates_restore_policy(self) -> bool {
        !matches!(self, Self::Restore | Self::Hotplug)
    }
}
