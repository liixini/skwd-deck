#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ApplySource {
    User,
    UserOverride,
    Random,
    Rotation,
    Playlist,
    Weather { playlist_id: i64, revision: u64 },
    WeatherSemantic { revision: u64 },
    Schedule,
    Workspace,
    Restore,
    OutdoorsRestore { revision: u64 },
    Hotplug,
    Replay,
}

impl ApplySource {
    pub(crate) fn manual_choice(self) -> bool {
        matches!(self, Self::User | Self::UserOverride | Self::Replay | Self::Random)
    }

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
