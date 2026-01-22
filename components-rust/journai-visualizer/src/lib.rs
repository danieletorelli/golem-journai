use common_lib::Visualizer;
use golem_rust::agent_implementation;

struct VisualizerImpl {
    name: String,
}

#[agent_implementation]
impl Visualizer for VisualizerImpl {
    fn new(name: String) -> Self {
        Self { name }
    }
}
