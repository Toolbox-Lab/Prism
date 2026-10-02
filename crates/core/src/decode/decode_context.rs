use crate::network::config::NetworkConfig;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Human,
    Json,
    Compact,
    Short,
}

impl OutputFormat {
    pub fn parse(s: &str) -> Self {
        match s {
            "json" => Self::Json,
            "compact" => Self::Compact,
            "short" => Self::Short,
            _ => Self::Human,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Json => "json",
            Self::Compact => "compact",
            Self::Short => "short",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DecodeContext {
    pub network: NetworkConfig,
    pub verbosity: u8,
    pub output_format: OutputFormat,
}

impl DecodeContext {
    pub fn builder() -> DecodeContextBuilder {
        DecodeContextBuilder::default()
    }
}

#[derive(Debug, Default)]
pub struct DecodeContextBuilder {
    network: Option<NetworkConfig>,
    verbosity: u8,
    output_format: OutputFormat,
}

impl DecodeContextBuilder {
    pub fn network(mut self, network: NetworkConfig) -> Self {
        self.network = Some(network);
        self
    }

    pub fn verbosity(mut self, verbosity: u8) -> Self {
        self.verbosity = verbosity;
        self
    }

    pub fn output_format(mut self, format: OutputFormat) -> Self {
        self.output_format = format;
        self
    }

    pub fn build(self) -> DecodeContext {
        DecodeContext {
            network: self.network.unwrap_or_else(NetworkConfig::testnet),
            verbosity: self.verbosity,
            output_format: self.output_format,
        }
    }
}

impl From<&NetworkConfig> for DecodeContextBuilder {
    fn from(network: &NetworkConfig) -> Self {
        Self::default().network(network.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Network;

    #[test]
    fn builder_defaults_to_testnet() {
        let ctx = DecodeContext::builder().build();
        assert_eq!(ctx.network.network, Network::Testnet);
        assert_eq!(ctx.verbosity, 0);
        assert_eq!(ctx.output_format, OutputFormat::Human);
    }

    #[test]
    fn builder_sets_all_fields() {
        let ctx = DecodeContext::builder()
            .network(NetworkConfig::mainnet())
            .verbosity(2)
            .output_format(OutputFormat::Json)
            .build();

        assert_eq!(ctx.network.network, Network::Mainnet);
        assert_eq!(ctx.verbosity, 2);
        assert_eq!(ctx.output_format, OutputFormat::Json);
    }

    #[test]
    fn output_format_roundtrip() {
        for (s, expected) in [
            ("human", OutputFormat::Human),
            ("json", OutputFormat::Json),
            ("compact", OutputFormat::Compact),
            ("short", OutputFormat::Short),
            ("unknown", OutputFormat::Human),
        ] {
            assert_eq!(OutputFormat::parse(s), expected);
            if s != "unknown" {
                assert_eq!(expected.as_str(), s);
            }
        }
    }

    #[test]
    fn builder_from_network_config_ref() {
        let network = NetworkConfig::mainnet();
        let ctx = DecodeContextBuilder::from(&network).build();
        assert_eq!(ctx.network.network, Network::Mainnet);
    }
}
