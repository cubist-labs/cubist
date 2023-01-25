#[cfg(test)]
mod commad_tests {
    use crate::commands::compile::compile;
    use crate::commands::new;
    use crate::commands::pre_compile::pre_compile;
    use cubist_config::{paths::Paths, *};
    use std::{collections::HashMap, fs, path::PathBuf};
    use tempfile::tempdir;

    fn new_simple_solc_project(
        tmp: &PathBuf,
        eth_sender: &str,
        eth_util: &str,
        poly_sender: &str,
        ava_receiver: &str,
    ) -> Config {
        // create new project
        fs::create_dir_all(tmp).unwrap();
        new::empty("my-app", ProjType::JavaScript, tmp, false).unwrap();
        let app_dir = tmp.join("my-app");

        // write the contracts to filesystem
        assert!(fs::create_dir_all(app_dir.join("contracts")).is_ok());
        assert!(fs::write(
            app_dir.join("contracts").join("PolySender.sol"),
            poly_sender
        )
        .is_ok());
        assert!(fs::write(app_dir.join("contracts").join("EthSender.sol"), eth_sender).is_ok());
        assert!(fs::write(app_dir.join("contracts").join("EthUtil.sol"), eth_util).is_ok());
        assert!(fs::write(
            app_dir.join("contracts").join("AvaReceiver.sol"),
            ava_receiver
        )
        .is_ok());

        // Read the config file as JSON
        assert!(fs::write(
            app_dir.join("cubist-config.json"),
            serde_json::json!({
                "type": "JavaScript",
                "build_dir": "build",
                "deploy_dir": "deploy",
                "contracts": {
                    "root_dir": "contracts",
                    "targets": {
                        "ethereum": {
                            "files": [ "./contracts/**/Eth*.sol" ],
                        },
                        "avalanche": {
                            "files": [ "./contracts/AvaReceiver.sol" ],
                        },
                        "polygon": {
                            "files": [ "./contracts/PolySender.sol" ],
                        }
                    }
                },
                "network_profiles": {
                    "default": {}
                },
                "current_network_profile": "default",
                "bridge_provider": "Cubist"
            })
            .to_string()
        )
        .is_ok());

        Config::from_file(app_dir.join("cubist-config.json")).unwrap()
    }

    static AVA_RECEIVER_CONTRACT: &str = r#"
        // SPDX-License-Identifier: UNLICENSED
        pragma solidity >=0.8.16;
        contract AvaReceiver {
            uint256 number;
            constructor() { number = 0; }
            function bar(uint256 num) public { number = num; }
        }"#;

    static ETH_UTIL_CONTRACT: &str = r#"
        // SPDX-License-Identifier: UNLICENSED
        pragma solidity >=0.8.16;
        contract EthUtil {
            function baz() public {}
        }
    "#;

    static ETH_SENDER_CONTRACT: &str = r#"
        // SPDX-License-Identifier: UNLICENSED
        pragma solidity >=0.8.16;
        contract EthSender {
            AvaReceiver rcv;
            EthUtil util;
            function foo(uint256 num) public { rcv.bar(num); util.baz(); }
        }"#;

    static POLY_SENDER_CONTRACT: &str = r#"
        // SPDX-License-Identifier: UNLICENSED
        pragma solidity >=0.8.16;
        contract PolySender {
            AvaReceiver rcv;
            EthUtil util;
            function poly_foo() public { util.baz(); }
            function foo(uint256 num) public { rcv.bar(num); util.baz(); }
        }"#;

    #[test]
    fn test_pre_compile_and_compile() {
        let tmp = tempdir().unwrap().into_path();
        let eth_util = String::from(ETH_UTIL_CONTRACT);
        let eth_sender = format!(
            "import './AvaReceiver.sol'; import './EthUtil.sol'; {}",
            ETH_SENDER_CONTRACT
        );
        let poly_sender = format!(
            "import './AvaReceiver.sol'; import './EthUtil.sol'; {}",
            POLY_SENDER_CONTRACT
        );
        let ava_receiver = AVA_RECEIVER_CONTRACT;

        let cfg = new_simple_solc_project(&tmp, &eth_sender, &eth_util, &poly_sender, ava_receiver);
        pre_compile(&cfg).unwrap_or_else(|err| {
            panic!("{:?}", err);
        });

        // make sure we copied/generated appropriate files per target
        let to_path =
            |target: Target, file| cfg.build_dir().join(target).join("contracts").join(file);
        let manifest_files = [Target::Avalanche, Target::Ethereum, Target::Polygon]
            .into_iter()
            .map(|target| {
                (
                    target,
                    PreCompileManifest::from_file(&Paths::new(&cfg).for_target(target).manifest)
                        .unwrap(),
                )
            })
            .collect::<HashMap<_, _>>();
        let get_manifest = |target| manifest_files.get(&target).unwrap();
        let assert_file =
            |path: &PathBuf| assert!(path.is_file(), "File '{}' not found", path.display());
        let assert_dir =
            |path: &PathBuf| assert!(path.is_dir(), "Directory '{}' not found", path.display());
        let assert_copied = |target, file, content| {
            let path = to_path(target, file);
            assert_file(&path);
            assert_eq!(content, fs::read_to_string(&path).unwrap());
            assert!(get_manifest(target)
                .files
                .iter()
                .filter(|f| !f.is_shim)
                .any(|f| f.rel_path == PathBuf::from(file) && !f.contract_dependencies.is_empty()));
        };
        let assert_generated = |target, file, wrong_content| {
            let path = to_path(target, file);
            assert_file(&path);
            assert_ne!(wrong_content, fs::read_to_string(&path).unwrap());
            assert_file(&path.with_extension("bridge.json"));
            assert!(get_manifest(target)
                .files
                .iter()
                .filter(|f| f.is_shim)
                .any(|f| f.rel_path == PathBuf::from(file) && !f.contract_dependencies.is_empty()));
        };
        let assert_compiler_output_layout = |target: Target| {
            assert_dir(&cfg.build_dir().join(target).join("artifacts"));
            assert_dir(&cfg.build_dir().join(target).join("build_infos"));
            assert_file(&cfg.build_dir().join(target).join("cache"));
        };
        let assert_compiler_artifact = |target: Target, file_name, contract_name| {
            let path = cfg
                .build_dir()
                .join(target)
                .join("artifacts")
                .join(file_name)
                .join(format!("{}.json", contract_name));
            assert_file(&path);
        };

        // avalanche/contracts/AvaReceiver.sol copied as is (since AvaReceiver.sol is on Avalanche)
        // avalanche/contracts/{EthSender,EthUtil,PolySender}.sol are NOT created (since AvaReceiver.sol doesn't use them)
        assert_copied(Target::Avalanche, "AvaReceiver.sol", AVA_RECEIVER_CONTRACT);

        // ethereum/contracts/{EthSender,EthUtil}.sol are COPIED as is (since both are on Ethereum)
        // ethereum/contracts/AvaReceiver.{sol,bridge.json} are GENERATED (since AvaReceiver is imported from EthSender)
        // ethereum/contracts/PolySender.sol is NOT created (since it's not imported)
        assert_copied(Target::Ethereum, "EthSender.sol", &eth_sender);
        assert_copied(Target::Ethereum, "EthUtil.sol", &eth_util);
        assert_generated(Target::Ethereum, "AvaReceiver.sol", AVA_RECEIVER_CONTRACT);

        // polygon/contracts/PolySender.sol is COPIED (since it's on Polygon)
        // polygon/contracts/EthUtil.{sol,bridge.json} is GENERATED (since it is imported from PolySender)
        // polygon/contracts/{AvaReceiver,EthSender}.sol are NOT created (since they are not imported)
        assert_copied(Target::Polygon, "PolySender.sol", &poly_sender);
        assert_generated(Target::Polygon, "EthUtil.sol", &eth_util);
        assert_generated(Target::Polygon, "AvaReceiver.sol", AVA_RECEIVER_CONTRACT);

        // now compile with solc
        compile(&cfg).unwrap_or_else(|err| {
            panic!("{:?}", err);
        });

        assert_compiler_output_layout(Target::Avalanche);
        assert_compiler_output_layout(Target::Ethereum);
        assert_compiler_output_layout(Target::Polygon);
        assert_compiler_artifact(Target::Avalanche, "AvaReceiver.sol", "AvaReceiver");
        assert_compiler_artifact(Target::Ethereum, "AvaReceiver.sol", "AvaReceiver");
        assert_compiler_artifact(Target::Ethereum, "EthSender.sol", "EthSender");
        assert_compiler_artifact(Target::Ethereum, "EthUtil.sol", "EthUtil");
        assert_compiler_artifact(Target::Polygon, "PolySender.sol", "PolySender");
        assert_compiler_artifact(Target::Polygon, "EthUtil.sol", "EthUtil");
    }
}
