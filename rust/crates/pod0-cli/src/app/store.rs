use std::path::Path;
use std::sync::Arc;

use pod0_facade::Pod0Facade;

use super::Shell;
use super::host_loop::HostPump;
use crate::mapping::open_error;
use crate::protocol::{CliError, ResponseData};

impl Shell {
    pub(super) fn create_store(&mut self, path: String) -> Result<ResponseData, CliError> {
        if Path::new(&path).exists() {
            return Err(CliError::new(
                "store_exists",
                "refusing to replace an existing store",
                false,
            ));
        }
        let facade = Pod0Facade::create(path.clone()).map_err(open_error)?;
        self.install_store(facade, path.clone())?;
        Ok(ResponseData::Store {
            path,
            created: true,
        })
    }

    pub(super) fn open_store(&mut self, path: String) -> Result<ResponseData, CliError> {
        let facade = Pod0Facade::open(path.clone()).map_err(open_error)?;
        self.install_store(facade, path.clone())?;
        Ok(ResponseData::Store {
            path,
            created: false,
        })
    }

    fn install_store(&mut self, facade: Arc<Pod0Facade>, path: String) -> Result<(), CliError> {
        self.stop_host_pump();
        let pump = HostPump::start(Arc::clone(&facade), Arc::clone(&self.host))?;
        self.facade = Some(facade);
        self.store_path = Some(path);
        self.host_pump = Some(pump);
        Ok(())
    }

    pub(super) fn stop_host_pump(&mut self) {
        if let Some(mut pump) = self.host_pump.take() {
            pump.shutdown();
        }
    }
}

impl Drop for Shell {
    fn drop(&mut self) {
        self.stop_host_pump();
    }
}
