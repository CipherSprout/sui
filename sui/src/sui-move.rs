// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use colored::Colorize;
use move_unit_test::UnitTestingConfig;
use std::path::Path;
use structopt::clap::App;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub enum MoveCommands {
    /// Build and verify Move project
    #[structopt(name = "build")]
    Build,

    /// Run all Move unit tests
    #[structopt(name = "test")]
    Test(UnitTestingConfig),
}

impl MoveCommands {
    pub fn execute(
        &self,
        path: &Path,
        is_std_framework: bool,
        output_hex: bool,
    ) -> Result<(), anyhow::Error> {
        match self {
            Self::Build => {
                if output_hex {
                    let compiled_modules = Self::print_hex(path, is_std_framework)?;
                    println!("{:?}", compiled_modules);
                } else {
                    Self::build(path, is_std_framework)?;
                    println!("Artifacts path: {:?}", path.join("build"));
                }
                println!("{}", "Build Successful".bold().green());
            }
            Self::Test(config) => {
                Self::build(path, is_std_framework)?;
                sui_framework::run_move_unit_tests(path, Some(config.clone()))?;
            }
        }
        Ok(())
    }

    fn print_hex(path: &Path, is_std_framework: bool) -> Result<Vec<String>, anyhow::Error> {
        Ok(sui_framework::build_move_package_to_hex(
            path,
            is_std_framework,
        )?)
    }

    fn build(path: &Path, is_std_framework: bool) -> Result<(), anyhow::Error> {
        if is_std_framework {
            sui_framework::get_sui_framework_modules(path)?;
        } else {
            sui_framework::build_and_verify_user_package(path)?;
        }
        Ok(())
    }
}

#[derive(StructOpt)]
#[structopt(
    name = "Sui Move Development Tool",
    about = "Tool to build and test Move applications",
    rename_all = "kebab-case"
)]
struct MoveOpt {
    /// Path to the Move project root.
    #[structopt(long, default_value = "./")]
    path: String,
    /// Whether we are building/testing the std/framework code.
    #[structopt(long)]
    std: bool,
    /// Whether we are printing in hex.
    #[structopt(long)]
    hex: bool,
    /// Subcommands.
    #[structopt(subcommand)]
    cmd: MoveCommands,
}

fn main() -> Result<(), anyhow::Error> {
    let app: App = MoveOpt::clap();
    let options = MoveOpt::from_clap(&app.get_matches());
    let path = options.path;
    options.cmd.execute(path.as_ref(), options.std, options.hex)
}
