use golem_rust::{agent_definition, agent_implementation};

#[agent_definition]
pub trait Analyzer {
    fn new(name: String) -> Self;
}

struct AnalyzerImpl {
    _name: String,
}

#[agent_implementation]
impl Analyzer for AnalyzerImpl {
    fn new(name: String) -> Self {
        Self { _name: name }
    }
}
