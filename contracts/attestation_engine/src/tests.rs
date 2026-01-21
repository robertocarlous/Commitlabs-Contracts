#![cfg(test)]

use super::*;
use soroban_sdk::{
    contract, contractimpl, contracttype, testutils::Address as _, testutils::Ledger as _, Address,
    Env, Map, String,
};

#[contracttype]
#[derive(Clone)]
enum CoreKey {
    Commitment(String),
    Violations(String),
}

#[contract]
struct MockCoreContract;

#[contractimpl]
impl MockCoreContract {
    pub fn set_commitment(e: Env, commitment_id: String, commitment: Commitment) {
        e.storage()
            .persistent()
            .set(&CoreKey::Commitment(commitment_id), &commitment);
    }

    pub fn set_violations(e: Env, commitment_id: String, has_violations: bool) {
        e.storage()
            .persistent()
            .set(&CoreKey::Violations(commitment_id), &has_violations);
    }

    pub fn get_commitment(e: Env, commitment_id: String) -> Commitment {
        e.storage()
            .persistent()
            .get(&CoreKey::Commitment(commitment_id))
            .expect("missing commitment")
    }

    pub fn check_violations(e: Env, commitment_id: String) -> bool {
        e.storage()
            .persistent()
            .get(&CoreKey::Violations(commitment_id))
            .unwrap_or(false)
    }
}

#[test]
fn test_initialize() {
    let e = Env::default();
    let admin = Address::generate(&e);
    let commitment_core_id = e.register_contract(None, MockCoreContract);
    let _contract_id = e.register_contract(None, AttestationEngineContract);
    
    e.as_contract(&_contract_id, || {
        AttestationEngineContract::initialize(e.clone(), admin, commitment_core_id);
    });
}

#[test]
fn test_attest() {
    let e = Env::default();
    let verified_by = Address::generate(&e);
    let core_id = e.register_contract(None, MockCoreContract);
    let _contract_id = e.register_contract(None, AttestationEngineContract);

    e.as_contract(&_contract_id, || {
        AttestationEngineContract::initialize(e.clone(), Address::generate(&e), core_id.clone());
    });

    let commitment_id = String::from_str(&e, "c1");
    let owner = Address::generate(&e);

    let rules = CommitmentRules {
        duration_days: 10,
        max_loss_percent: 20,
        commitment_type: String::from_str(&e, "safe"),
        early_exit_penalty: 0,
        min_fee_threshold: 0,
    };
    let commitment = Commitment {
        commitment_id: commitment_id.clone(),
        owner,
        nft_token_id: 1,
        rules,
        amount: 1_000,
        asset_address: Address::generate(&e),
        created_at: 0,
        expires_at: 100,
        current_value: 1_000,
        status: String::from_str(&e, "active"),
    };

    e.as_contract(&core_id, || {
        MockCoreContract::set_commitment(e.clone(), commitment_id.clone(), commitment);
        MockCoreContract::set_violations(e.clone(), commitment_id.clone(), false);
    });
    
    let data = Map::<String, String>::new(&e);
    e.as_contract(&_contract_id, || {
        AttestationEngineContract::attest(
            e.clone(),
            commitment_id.clone(),
            String::from_str(&e, "health_check"),
            data,
            verified_by,
        );
    });

    let atts = e.as_contract(&_contract_id, || {
        AttestationEngineContract::get_attestations(e.clone(), commitment_id)
    });
    assert!(atts.len() == 1);
}

#[test]
fn test_verify_compliance() {
    let e = Env::default();
    // Set a deterministic ledger timestamp for duration checks.
    e.ledger().with_mut(|li| {
        li.timestamp = 50;
    });

    let core_id = e.register_contract(None, MockCoreContract);
    let _contract_id = e.register_contract(None, AttestationEngineContract);
    e.as_contract(&_contract_id, || {
        AttestationEngineContract::initialize(e.clone(), Address::generate(&e), core_id.clone());
    });

    let commitment_id = String::from_str(&e, "c1");
    let owner = Address::generate(&e);

    let base_rules = CommitmentRules {
        duration_days: 10,
        max_loss_percent: 20,
        commitment_type: String::from_str(&e, "safe"),
        early_exit_penalty: 0,
        min_fee_threshold: 100,
    };

    // Happy path: in-range drawdown, not expired, fees meet threshold, no violations.
    let mut commitment = Commitment {
        commitment_id: commitment_id.clone(),
        owner: owner.clone(),
        nft_token_id: 1,
        rules: base_rules.clone(),
        amount: 1_000,
        asset_address: Address::generate(&e),
        created_at: 0,
        expires_at: 100,
        current_value: 900, // 10% drawdown
        status: String::from_str(&e, "active"),
    };
    e.as_contract(&core_id, || {
        MockCoreContract::set_commitment(e.clone(), commitment_id.clone(), commitment.clone());
        MockCoreContract::set_violations(e.clone(), commitment_id.clone(), false);
    });
    e.as_contract(&_contract_id, || {
        AttestationEngineContract::record_fees(e.clone(), commitment_id.clone(), 100);
    });

    assert!(e.as_contract(&_contract_id, || {
        AttestationEngineContract::verify_compliance(e.clone(), commitment_id.clone())
    }));

    // Loss limit exceeded
    commitment.current_value = 700; // 30% drawdown
    e.as_contract(&core_id, || {
        MockCoreContract::set_commitment(e.clone(), commitment_id.clone(), commitment.clone());
    });
    assert!(!e.as_contract(&_contract_id, || {
        AttestationEngineContract::verify_compliance(e.clone(), commitment_id.clone())
    }));

    // Duration expired
    commitment.current_value = 900;
    commitment.expires_at = 40;
    e.as_contract(&core_id, || {
        MockCoreContract::set_commitment(e.clone(), commitment_id.clone(), commitment.clone());
    });
    assert!(!e.as_contract(&_contract_id, || {
        AttestationEngineContract::verify_compliance(e.clone(), commitment_id.clone())
    }));

    // Fee threshold not met
    commitment.expires_at = 100;
    e.as_contract(&core_id, || {
        MockCoreContract::set_commitment(e.clone(), commitment_id.clone(), commitment.clone());
    });
    // Reset engine fees by using a new commitment id
    let commitment_id2 = String::from_str(&e, "c2");
    commitment.commitment_id = commitment_id2.clone();
    e.as_contract(&core_id, || {
        MockCoreContract::set_commitment(e.clone(), commitment_id2.clone(), commitment.clone());
        MockCoreContract::set_violations(e.clone(), commitment_id2.clone(), false);
    });
    assert!(!e.as_contract(&_contract_id, || {
        AttestationEngineContract::verify_compliance(e.clone(), commitment_id2.clone())
    }));

    // Active violations
    e.as_contract(&core_id, || {
        MockCoreContract::set_violations(e.clone(), commitment_id2.clone(), true);
    });
    assert!(!e.as_contract(&_contract_id, || {
        AttestationEngineContract::verify_compliance(e.clone(), commitment_id2)
    }));
    
    // Edge: duration_days == 0 bypasses duration check
    let commitment_id3 = String::from_str(&e, "c3");
    let rules_no_duration = CommitmentRules {
        duration_days: 0,
        ..base_rules
    };
    let commitment3 = Commitment {
        commitment_id: commitment_id3.clone(),
        owner,
        nft_token_id: 3,
        rules: rules_no_duration,
        amount: 0, // edge: amount==0 -> drawdown=0
        asset_address: Address::generate(&e),
        created_at: 0,
        expires_at: 0,
        current_value: 0,
        status: String::from_str(&e, "active"),
    };
    e.as_contract(&core_id, || {
        MockCoreContract::set_commitment(e.clone(), commitment_id3.clone(), commitment3);
        MockCoreContract::set_violations(e.clone(), commitment_id3.clone(), false);
    });
    // fees not met but threshold is 100 -> still should fail; make threshold 0
    let mut commitment3b = e.as_contract(&core_id, || {
        MockCoreContract::get_commitment(e.clone(), commitment_id3.clone())
    });
    commitment3b.rules.min_fee_threshold = 0;
    e.as_contract(&core_id, || {
        MockCoreContract::set_commitment(e.clone(), commitment_id3.clone(), commitment3b);
    });
    assert!(e.as_contract(&_contract_id, || {
        AttestationEngineContract::verify_compliance(e.clone(), commitment_id3)
    }));
}

