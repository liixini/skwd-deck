mod hub;

pub(crate) use hub::{EventHub, SUBSCRIBER_CHANNEL_CAPACITY};

#[cfg(test)]
mod tests;
