use clap::Parser;
use serde::Deserialize;

#[derive(Parser)]
#[command(name = "oj", version = "1.0", about, long_about = None)]
pub struct CliArgs {
    /// Path to the configuration file
    #[arg(long = "config", short = 'c')]
    pub config_path: String,

    /// Whether to flush the existing database
    #[arg(long = "flush-data", short = 'f', default_value_t = false)]
    pub flush_data: bool,
}

impl CliArgs {
    /// Load the configuration from the specified file
    pub fn to_config(&self) -> std::io::Result<Config> {
        let file = std::fs::File::open(&self.config_path)?;
        let reader = std::io::BufReader::new(file);
        serde_json::from_reader(reader).map_err(|e| e.into())
    }
}

#[derive(Deserialize, Debug)]
pub struct Config {
    pub server: ServerConfig,
    pub problems: Vec<ProblemConfig>,
    pub languages: Vec<LanguageConfig>,
}

#[derive(Deserialize, Debug)]
pub struct ServerConfig {
    pub bind_address: Option<String>,
    pub bind_port: Option<u16>,
}

#[derive(Deserialize, Debug)]
pub struct ProblemConfig {
    pub id: u32,
    pub name: String,
    #[serde(flatten)]
    pub judge_type: JudgeType,
    pub nonblocking: Option<bool>,
    pub misc: Option<serde_json::Value>,
    pub cases: Vec<ProblemCaseConfig>,
}

#[derive(Deserialize, Debug)]
pub struct ProblemCaseConfig {
    pub score: f32,
    pub input_file: String,
    pub answer_file: String,
    pub time_limit: MicroSecond,
    pub memory_limit: ByteSize,
}

#[derive(Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MicroSecond(pub u64);

#[derive(Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ByteSize(pub u64);

#[derive(Deserialize, Debug)]
pub struct LanguageConfig {
    pub name: String,
    pub file_name: String,
    pub command: Vec<String>,
}

#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JudgeType {
    Standard,
    Strict,
    Spj,
    DynamicRanking,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_deserialization() {
        let file = std::fs::File::open("data/example.json").unwrap();
        let reader = std::io::BufReader::new(file);
        let config: Config = serde_json::from_reader(reader).unwrap();
        assert_eq!(config.server.bind_address, Some("127.0.0.1".to_string()));
        assert_eq!(config.problems[0].judge_type, JudgeType::Standard);
        assert_eq!(config.problems[0].cases[0].time_limit, MicroSecond(1000000));
    }
}
