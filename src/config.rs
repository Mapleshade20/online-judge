use clap::Parser;
use serde::Deserialize;

#[derive(Parser)]
#[command(name = "oj", version = "0.1.0", about, long_about = None)]
pub struct CliArgs {
    /// Path to the configuration file
    #[arg(long = "config", short = 'c')]
    pub config_path: String,

    /// Whether to remove the existing database
    #[arg(long = "flush-data", short = 'f')]
    pub flush_data: bool,

    /// Number of threads to judge concurrently
    #[arg(short, long, default_value_t = 2)]
    pub threads: u8,

    /// Verbose logging
    #[arg(short, long)]
    pub verbose: bool,
}

impl CliArgs {
    /// Load the configuration from the specified file
    pub fn read_config(&self) -> std::io::Result<Config> {
        let file = std::fs::File::open(&self.config_path)?;
        let reader = std::io::BufReader::new(file);
        serde_json::from_reader(reader).map_err(|e| e.into())
    }
}

#[derive(Deserialize, Debug)]
pub struct Config {
    pub server: ServerConfig,
    pub problems: ProblemConfig,
    pub languages: LanguageConfig,
}

#[derive(Deserialize, Debug)]
pub struct ServerConfig {
    pub bind_address: Option<String>,
    pub bind_port: Option<u16>,
    pub blocking: Option<bool>,
}

pub type ProblemConfig = Vec<OneProblemConfig>;
pub type LanguageConfig = Vec<OneLanguageConfig>;

#[derive(Deserialize, Debug, Clone)]
pub struct OneProblemConfig {
    pub id: u32,
    pub name: String,
    #[serde(flatten)]
    pub judge_type: JudgeType,
    // pub misc: Option<serde_json::Value>,
    pub cases: Vec<OneCaseConfig>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct OneCaseConfig {
    pub score: f64,
    pub input_file: String,
    pub answer_file: String,
    pub time_limit: MicroSecond,
    pub memory_limit: KiloByte,
}

#[derive(Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct MicroSecond(pub u32);

#[derive(Deserialize, Debug, PartialEq, PartialOrd, Clone, Copy)]
pub struct Second(pub f64);

impl From<MicroSecond> for Second {
    fn from(value: MicroSecond) -> Self {
        Second(value.0 as f64 / 1_000_000.0)
    }
}

impl From<Second> for MicroSecond {
    fn from(value: Second) -> Self {
        MicroSecond((value.0 * 1_000_000.0) as u32)
    }
}

#[derive(Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct KiloByte(pub u32);

#[derive(Deserialize, Debug, Clone)]
pub struct OneLanguageConfig {
    pub name: String,
    pub file_name: String,
    pub command: Vec<String>,
}

#[derive(Deserialize, Debug, PartialEq, Eq, Clone)]
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
