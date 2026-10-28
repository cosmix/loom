//! Token usage of a turn, counted once per message id.

use std::collections::HashMap;

use serde_json::Value;

use crate::plan_session::model::TokenCounts;
use crate::provider_data::claude;

#[derive(Default)]
pub(crate) struct MessageUsage {
    messages: HashMap<String, TokenCounts>,
}

impl MessageUsage {
    pub(crate) fn record(&mut self, message_id: &str, usage: &Value) {
        if !usage.is_object() || claude::invalid_usage(usage) {
            return;
        }
        let (usage, _) = claude::parse(usage);
        self.messages.insert(
            message_id.to_owned(),
            TokenCounts {
                input: usage.input,
                cache_write: usage.cache_creation,
                cache_read: usage.cache_read,
                output: usage.output,
            },
        );
    }

    pub(crate) fn total(&self) -> TokenCounts {
        self.messages
            .values()
            .fold(TokenCounts::default(), |mut total, usage| {
                total.input = total.input.saturating_add(usage.input);
                total.cache_write = total.cache_write.saturating_add(usage.cache_write);
                total.cache_read = total.cache_read.saturating_add(usage.cache_read);
                total.output = total.output.saturating_add(usage.output);
                total
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn usage(input: u64, cache_write: u64, cache_read: u64, output: u64) -> Value {
        json!({
            "input_tokens": input,
            "cache_creation_input_tokens": cache_write,
            "cache_read_input_tokens": cache_read,
            "output_tokens": output,
        })
    }

    #[test]
    fn last_record_for_one_message_counts_once() {
        let mut messages = MessageUsage::default();

        messages.record("msg-1", &usage(10, 20, 30, 40));
        messages.record("msg-1", &usage(50, 60, 70, 80));
        messages.record("msg-1", &usage(1, 2, 3, 4));

        assert_eq!(
            messages.total(),
            TokenCounts {
                input: 1,
                cache_write: 2,
                cache_read: 3,
                output: 4,
            }
        );
    }

    #[test]
    fn two_message_ids_add_up() {
        let mut messages = MessageUsage::default();

        messages.record("msg-1", &usage(1, 2, 3, 4));
        messages.record("msg-2", &usage(10, 20, 30, 40));

        assert_eq!(
            messages.total(),
            TokenCounts {
                input: 11,
                cache_write: 22,
                cache_read: 33,
                output: 44,
            }
        );
    }

    #[test]
    fn invalid_usage_does_not_replace_or_add_a_message() {
        let mut messages = MessageUsage::default();
        messages.record("msg-1", &usage(1, 2, 3, 4));

        messages.record("msg-1", &json!({"input_tokens": -1, "output_tokens": 100}));
        messages.record(
            "msg-2",
            &json!({"input_tokens": 100, "output_tokens": "bad"}),
        );
        messages.record("msg-1", &Value::Null);

        assert_eq!(
            messages.total(),
            TokenCounts {
                input: 1,
                cache_write: 2,
                cache_read: 3,
                output: 4,
            }
        );
    }

    #[test]
    fn empty_usage_has_zero_total() {
        assert_eq!(MessageUsage::default().total(), TokenCounts::default());
    }

    #[test]
    fn cache_creation_split_is_normalized() {
        let mut messages = MessageUsage::default();

        messages.record(
            "msg-1",
            &json!({
                "cache_creation": {
                    "ephemeral_5m_input_tokens": 2,
                    "ephemeral_1h_input_tokens": 3,
                },
            }),
        );

        assert_eq!(
            messages.total(),
            TokenCounts {
                cache_write: 5,
                ..TokenCounts::default()
            }
        );
    }

    #[test]
    fn totals_saturate_instead_of_overflowing() {
        let mut messages = MessageUsage::default();

        messages.record("msg-1", &usage(u64::MAX, u64::MAX, u64::MAX, u64::MAX));
        messages.record("msg-2", &usage(1, 1, 1, 1));

        assert_eq!(
            messages.total(),
            TokenCounts {
                input: u64::MAX,
                cache_write: u64::MAX,
                cache_read: u64::MAX,
                output: u64::MAX,
            }
        );
    }
}
