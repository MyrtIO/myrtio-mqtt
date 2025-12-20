//! Topic registration and collection for MQTT modules.

use heapless::{String, Vec};

use super::traits::TopicCollector;

/// Maximum length for a single topic string.
pub const MAX_TOPIC_LEN: usize = 128;

/// A registry for topics that modules want to subscribe to.
///
/// This registry owns the topic strings (copies them on add), making it
/// suitable for use with the object-safe `TopicCollector` trait.
///
/// # Example
///
/// ```ignore
/// let mut registry = TopicRegistry::<8>::new();
///
/// // Via TopicCollector trait
/// module.register(&mut registry);
///
/// // Iterate and subscribe
/// for topic in registry.iter() {
///     client.subscribe(topic, QoS::AtMostOnce).await?;
/// }
/// ```
#[derive(Default)]
pub struct TopicRegistry<const MAX_TOPICS: usize> {
    topics: Vec<String<MAX_TOPIC_LEN>, MAX_TOPICS>,
}

impl<const MAX_TOPICS: usize> TopicRegistry<MAX_TOPICS> {
    /// Create a new empty topic registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a topic to the registry by copying the string.
    ///
    /// Returns `true` if successful, `false` if the registry is full
    /// or the topic is too long.
    pub fn add_topic(&mut self, topic: &str) -> bool {
        if topic.len() > MAX_TOPIC_LEN {
            return false;
        }

        let mut owned = String::new();
        if owned.push_str(topic).is_err() {
            return false;
        }

        self.topics.push(owned).is_ok()
    }

    /// Get an iterator over the registered topics.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.topics.iter().map(|s| s.as_str())
    }

    /// Get the number of registered topics.
    pub fn len(&self) -> usize {
        self.topics.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.topics.is_empty()
    }

    /// Clear all registered topics.
    pub fn clear(&mut self) {
        self.topics.clear();
    }
}

impl<const MAX_TOPICS: usize> TopicCollector for TopicRegistry<MAX_TOPICS> {
    fn add(&mut self, topic: &str) -> bool {
        self.add_topic(topic)
    }
}
