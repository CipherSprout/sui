// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_json_rpc::name_service::Domain;

#[test]
fn test_name_service_outputs() {
    assert_eq!("@test".parse::<Domain>().unwrap().to_string(), "test.sui");
    assert_eq!(
        "test.sui".parse::<Domain>().unwrap().to_string(),
        "test.sui"
    );
    assert_eq!(
        "test@sld".parse::<Domain>().unwrap().to_string(),
        "test.sld.sui"
    );
    assert_eq!(
        "test.test@example".parse::<Domain>().unwrap().to_string(),
        "test.test.example.sui"
    );
    assert_eq!(
        "sui@sui".parse::<Domain>().unwrap().to_string(),
        "sui.sui.sui"
    );

    assert_eq!("@sui".parse::<Domain>().unwrap().to_string(), "sui.sui");

    assert_eq!(
        "test*test@test".parse::<Domain>().unwrap().to_string(),
        "test.test.test.sui"
    );
}

#[test]
fn test_different_wildcard() {
    assert_eq!("test.sui".parse::<Domain>(), "test*sui".parse::<Domain>(),);
    assert_eq!("@test".parse::<Domain>(), "test*sui".parse::<Domain>(),);
}

#[test]
fn test_invalid_inputs() {
    assert!("*".parse::<Domain>().is_err());
    assert!(".".parse::<Domain>().is_err());
    assert!("@".parse::<Domain>().is_err());
    assert!("@inner.sui".parse::<Domain>().is_err());
    assert!("@inner*sui".parse::<Domain>().is_err());
    assert!("test@".parse::<Domain>().is_err());
    assert!("sui".parse::<Domain>().is_err());
    assert!("test.test@example.sui".parse::<Domain>().is_err());
    assert!("test@test@example".parse::<Domain>().is_err());
}

#[test]
fn base_format_tests() {
    let mut domain = "example.sui".parse::<Domain>().unwrap();
    assert_eq!(domain.format(&'.', false), "example.sui");
    domain = "test.example.sui".parse::<Domain>().unwrap();
    assert_eq!(domain.format(&'.', false), "test.example.sui");
    domain = "inner.test.example.sui".parse::<Domain>().unwrap();
    assert_eq!(domain.format(&'.', false), "inner.test.example.sui");
    domain = "more.inner.test.example.sui".parse::<Domain>().unwrap();
    assert_eq!(domain.format(&'.', false), "more.inner.test.example.sui");
}

#[test]
fn new_format_tests() {
    let mut domain = "example.sui".parse::<Domain>().unwrap();
    assert_eq!(domain.format(&'.', true), "@example");

    domain = "test.example.sui".parse::<Domain>().unwrap();
    assert_eq!(domain.format(&'.', true), "test@example");

    domain = "inner.test.example.sui".parse::<Domain>().unwrap();
    assert_eq!(domain.format(&'.', true), "inner.test@example");

    domain = "more.inner.test.example.sui".parse::<Domain>().unwrap();
    assert_eq!(domain.format(&'.', true), "more.inner.test@example");
}
