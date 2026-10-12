use rmcp::model::ProtocolVersion;

/// The newest MCP revision `pgs-mcp` speaks, as it appears on the wire.
///
/// Kept as a string for wire-level test assertions; `src/` itself uses
/// [`SUPPORTED_PROTOCOL_VERSIONS`] and never parses this back at runtime.
pub const PROTOCOL_VERSION: &str = "2026-07-28";

/// Every MCP revision `pgs-mcp` serves, oldest first.
///
/// `2026-07-28` removed `initialize`, but most clients still open a session
/// with it, so the server also serves the two `initialize`-era revisions.
/// `2025-06-18` is the floor because it introduced `outputSchema` and
/// `structuredContent`, which every pgs tool result relies on.
pub const SUPPORTED_PROTOCOL_VERSIONS: &[ProtocolVersion] = &[
    ProtocolVersion::V_2025_06_18,
    ProtocolVersion::V_2025_11_25,
    ProtocolVersion::V_2026_07_28,
];

pub mod contract;
mod runtime;
pub mod server;

#[cfg(test)]
mod tests {
    use super::{PROTOCOL_VERSION, SUPPORTED_PROTOCOL_VERSIONS};
    use rmcp::model::ProtocolVersion;

    #[test]
    fn protocol_version_string_is_the_newest_supported_revision() {
        assert_eq!(
            SUPPORTED_PROTOCOL_VERSIONS
                .last()
                .map(ProtocolVersion::as_str),
            Some(PROTOCOL_VERSION)
        );
    }
}
