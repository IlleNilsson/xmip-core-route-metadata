#![forbid(unsafe_code)]

//! The metadata route technology — a technology of `xmip-core-route`.
//!
//! A Subscription's filter names properties, and each property is read from
//! one source. This source reads what the Message knows about itself, none of
//! which is content and none of which is context: `metadata:generation`, how
//! many times content or metadata changed since Receive; `metadata:created-by`,
//! which of receive, assignment, transformation and send-preparation produced
//! this generation; `metadata:sections`, how many addressable parts it has;
//! `metadata:size`, the bytes across them; `metadata:priority` and
//! `metadata:durability`, the treatment it was declared with; and
//! `metadata:id`, its identifier in canonical UUID form. Every name always has
//! a value, so this source never promotes nothing; a name outside the set is an
//! error naming the set. ADR-0046.
//!
//! A route technology does not decide anything: it reads.

use message::{Message, MessageCreationSource, MessageDurability, MessagePriority};
use route::{Source, SourceError};

/// The manifest leaf and the prefix a property carries.
pub const TECHNOLOGY: &str = "metadata";

/// The names this technology reads, in the order the crate documentation
/// gives them.
pub const NAMES: [&str; 7] = [
    "generation",
    "created-by",
    "sections",
    "size",
    "priority",
    "durability",
    "id",
];

/// Reads what the Message knows about itself.
pub struct MetadataSource;

impl Source for MetadataSource {
    fn technology(&self) -> &'static str {
        TECHNOLOGY
    }

    fn read(&self, message: &Message, name: &str) -> Result<Option<String>, SourceError> {
        let value = match name {
            "generation" => message.generation().to_string(),
            "created-by" => created_by(message.created_by()).to_string(),
            "sections" => message.sections().len().to_string(),
            "size" => message
                .sections()
                .iter()
                .map(|section| section.stream.len())
                .sum::<usize>()
                .to_string(),
            "priority" => priority(message.treatment().priority).to_string(),
            "durability" => durability(message.treatment().durability).to_string(),
            "id" => message.message_id().to_string(),
            other => {
                return Err(SourceError::new(
                    TECHNOLOGY,
                    other,
                    format!(
                        "not a thing a Message knows about itself; the names are {}",
                        NAMES.join(", ")
                    ),
                ));
            }
        };
        Ok(Some(value))
    }
}

/// The word the artifact vocabulary uses for each creation source.
const fn created_by(source: MessageCreationSource) -> &'static str {
    match source {
        MessageCreationSource::Receive => "receive",
        MessageCreationSource::Assignment => "assignment",
        MessageCreationSource::Transformation => "transformation",
        MessageCreationSource::SendPreparation => "send-preparation",
    }
}

const fn priority(priority: MessagePriority) -> &'static str {
    match priority {
        MessagePriority::Immediate => "immediate",
        MessagePriority::High => "high",
        MessagePriority::Normal => "normal",
        MessagePriority::Low => "low",
        MessagePriority::Background => "background",
    }
}

const fn durability(durability: MessageDurability) -> &'static str {
    match durability {
        MessageDurability::Ephemeral => "ephemeral",
        MessageDurability::Durable => "durable",
        MessageDurability::Recoverable => "recoverable",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use context::MessageContext;
    use message::{ExecutionProfile, MessageSection, MessageTreatment};
    use route::{Predicate, Value};
    use stream::Stream;
    use xcore::{MessageId, SectionId, StreamId};

    fn section(id: u128, bytes: &[u8]) -> MessageSection {
        MessageSection {
            section_id: SectionId::new(id),
            name: None,
            stream: Stream::new(StreamId::new(id), bytes.to_vec(), None),
            contract: None,
        }
    }

    fn received() -> Message {
        Message::received(
            MessageId::new(7),
            vec![section(10, b"<order/>"), section(11, b"attachment")],
            MessageContext::new(),
            MessageTreatment::default(),
        )
    }

    fn read(message: &Message, name: &str) -> String {
        MetadataSource
            .read(message, name)
            .expect("readable")
            .expect("always a value")
    }

    #[test]
    fn a_received_message_knows_its_generation_origin_sections_and_size() {
        let message = received();
        assert_eq!(read(&message, "generation"), "0");
        assert_eq!(read(&message, "created-by"), "receive");
        assert_eq!(read(&message, "sections"), "2");
        assert_eq!(read(&message, "size"), "18");
        assert_eq!(read(&message, "id"), MessageId::new(7).to_string());
    }

    #[test]
    fn the_treatment_reads_as_the_words_an_artifact_declares() {
        let message = received().with_treatment(MessageTreatment {
            priority: MessagePriority::Background,
            execution_profile: ExecutionProfile::PassThrough,
            durability: MessageDurability::Ephemeral,
        });
        assert_eq!(read(&message, "priority"), "background");
        assert_eq!(read(&message, "durability"), "ephemeral");
        assert_eq!(read(&received(), "priority"), "normal");
        assert_eq!(read(&received(), "durability"), "recoverable");
    }

    #[test]
    fn a_later_generation_says_what_produced_it() {
        let assigned = received().assigned(MessageId::new(8), MessageContext::new());
        assert_eq!(read(&assigned, "generation"), "1");
        assert_eq!(read(&assigned, "created-by"), "assignment");

        let transformed = assigned.transformed(
            MessageId::new(9),
            vec![section(12, b"{}")],
            MessageContext::new(),
            MessageCreationSource::Transformation,
        );
        assert_eq!(read(&transformed, "generation"), "2");
        assert_eq!(read(&transformed, "created-by"), "transformation");
        assert_eq!(read(&transformed, "size"), "2");
    }

    #[test]
    fn a_name_outside_the_set_is_refused_naming_the_set() {
        let refused = MetadataSource
            .read(&received(), "colour")
            .expect_err("refused");
        assert_eq!(refused.technology, "metadata");
        assert_eq!(refused.property, "colour");
        assert!(refused.reason.contains("generation, created-by, sections"));
    }

    #[test]
    fn the_technology_is_metadata_and_promote_reads_the_prefixed_property() {
        assert_eq!(MetadataSource.technology(), "metadata");

        let sources: [&dyn Source; 1] = [&MetadataSource];
        let promoted = route::promote(
            &received(),
            &sources,
            &["metadata:size", "metadata:created-by"],
        )
        .expect("readable");

        assert!(
            Predicate::less_than("metadata:size", Value::Integer(1024))
                .test(&promoted)
                .passed()
        );
        assert!(
            Predicate::equals("metadata:created-by", Value::Text("receive".into()))
                .test(&promoted)
                .passed()
        );
    }
}
