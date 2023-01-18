use console::style;
use cubist_config::Config;
use cubist_sdk::core::CubistInfo;
use eyre::Result;

use crate::cube::CubeFactory;

pub fn gen_orm(cfg: Config) -> Result<()> {
    let cube = CubeFactory::create(cfg.type_, &cfg.project_dir())?;
    let cubist = CubistInfo::new(cfg)?;
    println!("{} ORM interface", style("Generating").bold().green());
    cube.gen_orm(&cubist)
}
