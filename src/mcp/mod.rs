/// The single MCP revision `pgs-mcp` speaks, as it appears on the wire.
///
/// Kept as a string for wire-level test assertions; `src/` itself uses
/// [`rmcp::model::ProtocolVersion::V_2026_07_28`] directly and never parses
/// this back into a protocol version at runtime.
pub const PROTOCOL_VERSION: &str = "2026-07-28";

pub mod contract;
mod runtime;
pub mod server;

#[cfg(test)]
mod tests {
    use super::PROTOCOL_VERSION;
    use rmcp::model::ProtocolVersion;

    #[test]
    fn protocol_version_string_matches_the_advertised_enum_variant() {
        assert_eq!(ProtocolVersion::V_2026_07_28.as_str(), PROTOCOL_VERSION);
    }
}
