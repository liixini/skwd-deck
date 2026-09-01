mod entry;
mod ring;

pub(crate) use entry::HistoryEntry;
pub(crate) use ring::OutputHistory;

#[cfg(test)]
mod tests;
