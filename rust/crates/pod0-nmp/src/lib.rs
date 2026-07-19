#![forbid(unsafe_code)]

/// Pod0's only direct owner of the generic NMP engine. Product-domain crates
/// do not receive the engine or its mechanism types.
pub struct NmpRuntime {
    engine: nmp::Engine,
}

impl NmpRuntime {
    /// Starts a non-persistent runtime for lifecycle/contract qualification.
    /// Product state is not written during bootstrap.
    pub fn start_in_memory() -> Result<Self, nmp::EngineError> {
        nmp::Engine::new(nmp::EngineConfig::default()).map(|engine| Self { engine })
    }

    /// Deterministic teardown belongs to the runtime owner and is explicit.
    pub fn shutdown(&self) {
        self.engine.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_facade_constructs_and_shuts_down() {
        let runtime = NmpRuntime::start_in_memory().expect("NMP runtime should start");
        runtime.shutdown();
    }
}
