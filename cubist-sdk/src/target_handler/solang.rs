use std::collections::HashMap;
use std::path::Path;

use crate::core::{CompileResult, ContractCompiler, ContractInfo};
use crate::Result;

pub struct SolangCompiler;

impl ContractCompiler for SolangCompiler {
    fn clean(&self) -> Result<()> {
        todo!()
    }

    fn compile_file(&self, _file: &Path) -> Result<CompileResult> {
        todo!()
    }

    fn find_compiled_contracts(
        &self,
        _source_file: &Path,
    ) -> Result<HashMap<String, ContractInfo>> {
        todo!();
    }
}
