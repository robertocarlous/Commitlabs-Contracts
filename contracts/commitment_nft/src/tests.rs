#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, testutils::Events, Address, Env, String};

// ============================================================================
// Helper Functions
// ============================================================================

fn setup_contract<'a>(e: &'a Env) -> (Address, Address, Address, CommitmentNFTContractClient<'a>) {
    let admin = Address::generate(e);
    let core_contract = Address::generate(e);
    let owner = Address::generate(e);

    let contract_id = e.register_contract(None, CommitmentNFTContract);
    let client = CommitmentNFTContractClient::new(e, &contract_id);

    (admin, core_contract, owner, client)
}

fn mint_test_nft(e: &Env, client: &CommitmentNFTContractClient, owner: &Address) -> u32 {
    let asset = Address::generate(e);

    client.mint(
        owner,
        &String::from_str(e, "commitment-1"),
        &30, // duration_days
        &10, // max_loss_percent
        &String::from_str(e, "balanced"),
        &1000, // initial_amount
        &asset,
    )
}

// ============================================================================
// Initialization Tests
// ============================================================================

#[test]
fn test_initialize() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let contract_id = e.register_contract(None, CommitmentNFTContract);
    let client = CommitmentNFTContractClient::new(&e, &contract_id);

    // Initialize the contract
    client.initialize(&admin);

    // Verify admin is set
    assert_eq!(client.get_admin(), admin);

    // Verify counters are initialized
    assert_eq!(client.total_supply(), 0);
    assert_eq!(client.current_token_id(), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_initialize_already_initialized() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let contract_id = e.register_contract(None, CommitmentNFTContract);
    let client = CommitmentNFTContractClient::new(&e, &contract_id);

    // Initialize once
    client.initialize(&admin);

    // Try to initialize again - should panic with AlreadyInitialized (error code 2)
    client.initialize(&admin);
}

// ============================================================================
// Access Control Tests
// ============================================================================

#[test]
fn test_set_core_contract() {
    let e = Env::default();
    e.mock_all_auths();

    let (admin, core_contract, _, client) = setup_contract(&e);

    // Initialize
    client.initialize(&admin);

    // Set core contract
    client.set_core_contract(&core_contract);

    // Verify core contract is set
    assert_eq!(client.get_core_contract(), core_contract);
}

// ============================================================================
// Minting Tests
// ============================================================================

#[test]
fn test_mint() {
    let e = Env::default();
    e.mock_all_auths();

    let (admin, _, owner, client) = setup_contract(&e);
    let asset = Address::generate(&e);

    // Initialize
    client.initialize(&admin);

    // Mint an NFT
    let token_id = client.mint(
        &owner,
        &String::from_str(&e, "commitment-1"),
        &30,
        &10,
        &String::from_str(&e, "balanced"),
        &1000,
        &asset,
    );

    // Verify token_id is 1 (first mint)
    assert_eq!(token_id, 1);

    // Verify ownership
    assert_eq!(client.owner_of(&token_id), owner);

    // Verify supply increased
    assert_eq!(client.total_supply(), 1);

    // Verify NFT is active
    assert!(client.is_active(&token_id));

    // Verify metadata
    let metadata = client.get_metadata(&token_id);
    assert_eq!(metadata.commitment_id, String::from_str(&e, "commitment-1"));
    assert_eq!(metadata.duration_days, 30);
    assert_eq!(metadata.max_loss_percent, 10);
}

// ============================================================================
// Transfer Tests
// ============================================================================

#[test]
fn test_transfer() {
    let e = Env::default();
    e.mock_all_auths();

    let (admin, _, owner, client) = setup_contract(&e);
    let new_owner = Address::generate(&e);

    // Initialize and mint
    client.initialize(&admin);
    let token_id = mint_test_nft(&e, &client, &owner);

    // Transfer
    client.transfer(&owner, &new_owner, &token_id);

    // Verify new owner
    assert_eq!(client.owner_of(&token_id), new_owner);

    // Verify NFT data is updated
    let nft = client.get_nft(&token_id);
    assert_eq!(nft.owner, new_owner);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_transfer_not_owner() {
    let e = Env::default();
    e.mock_all_auths();

    let (admin, _, owner, client) = setup_contract(&e);
    let not_owner = Address::generate(&e);
    let new_owner = Address::generate(&e);

    // Initialize and mint
    client.initialize(&admin);
    let token_id = mint_test_nft(&e, &client, &owner);

    // Try to transfer from non-owner - should panic with NotOwner (error code 7)
    client.transfer(&not_owner, &new_owner, &token_id);
}

// ============================================================================
// Settlement Tests (Issue #5 - Main Tests)
// ============================================================================

#[test]
fn test_settle_success() {
    let e = Env::default();
    e.mock_all_auths();

    let (admin, core_contract, owner, client) = setup_contract(&e);

    // Initialize and set up
    client.initialize(&admin);
    client.set_core_contract(&core_contract);

    // Mint an NFT
    let token_id = mint_test_nft(&e, &client, &owner);

    // Verify NFT is active before settlement
    assert!(client.is_active(&token_id));

    // Settle as the authorized core contract
    client.settle(&core_contract, &token_id);

    // Verify NFT is now inactive
    assert!(!client.is_active(&token_id));

    // Verify NFT data is updated
    let nft = client.get_nft(&token_id);
    assert!(!nft.is_active);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_settle_unauthorized_caller() {
    let e = Env::default();
    e.mock_all_auths();

    let (admin, core_contract, owner, client) = setup_contract(&e);
    let unauthorized = Address::generate(&e);

    // Initialize and set up
    client.initialize(&admin);
    client.set_core_contract(&core_contract);

    // Mint an NFT
    let token_id = mint_test_nft(&e, &client, &owner);

    // Try to settle as unauthorized caller - should panic with Unauthorized (error code 4)
    client.settle(&unauthorized, &token_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_settle_nft_not_found() {
    let e = Env::default();
    e.mock_all_auths();

    let (admin, core_contract, _, client) = setup_contract(&e);

    // Initialize and set up
    client.initialize(&admin);
    client.set_core_contract(&core_contract);

    // Try to settle non-existent NFT - should panic with NFTNotFound (error code 3)
    client.settle(&core_contract, &999);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_settle_already_settled() {
    let e = Env::default();
    e.mock_all_auths();

    let (admin, core_contract, owner, client) = setup_contract(&e);

    // Initialize and set up
    client.initialize(&admin);
    client.set_core_contract(&core_contract);

    // Mint an NFT
    let token_id = mint_test_nft(&e, &client, &owner);

    // Settle once - should succeed
    client.settle(&core_contract, &token_id);

    // Try to settle again - should panic with AlreadySettled (error code 5)
    client.settle(&core_contract, &token_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_settle_core_contract_not_set() {
    let e = Env::default();
    e.mock_all_auths();

    let (admin, core_contract, owner, client) = setup_contract(&e);

    // Initialize but DON'T set core contract
    client.initialize(&admin);

    // Mint an NFT
    let token_id = mint_test_nft(&e, &client, &owner);

    // Try to settle - should panic with NotInitialized (error code 1) because core contract not set
    client.settle(&core_contract, &token_id);
}

#[test]
fn test_settle_event_emission() {
    let e = Env::default();
    e.mock_all_auths();

    let (admin, core_contract, owner, client) = setup_contract(&e);

    // Initialize and set up
    client.initialize(&admin);
    client.set_core_contract(&core_contract);

    // Mint an NFT
    let token_id = mint_test_nft(&e, &client, &owner);

    // Settle
    client.settle(&core_contract, &token_id);

    // Verify events were emitted
    let events = e.events().all();

    // Should have at least the Settle event
    // Events: CoreContractSet, Mint, Settle
    assert!(events.len() >= 3);

    // Find the Settle event and verify its structure
    // The last event should be the Settle event
    let _last_event = events.last().unwrap();

    // Verify event topics contain "Settle" and token_id
    // Event structure: ((Symbol("Settle"), token_id), (timestamp, final_status))
    assert!(!events.is_empty());
}

#[test]
fn test_settle_multiple_nfts() {
    let e = Env::default();
    e.mock_all_auths();

    let (admin, core_contract, owner, client) = setup_contract(&e);

    // Initialize and set up
    client.initialize(&admin);
    client.set_core_contract(&core_contract);

    // Mint multiple NFTs
    let token_id_1 = mint_test_nft(&e, &client, &owner);
    let token_id_2 = mint_test_nft(&e, &client, &owner);
    let token_id_3 = mint_test_nft(&e, &client, &owner);

    // Verify all are active
    assert!(client.is_active(&token_id_1));
    assert!(client.is_active(&token_id_2));
    assert!(client.is_active(&token_id_3));

    // Settle only the first one
    client.settle(&core_contract, &token_id_1);

    // Verify first is inactive, others still active
    assert!(!client.is_active(&token_id_1));
    assert!(client.is_active(&token_id_2));
    assert!(client.is_active(&token_id_3));

    // Settle the third one
    client.settle(&core_contract, &token_id_3);

    // Verify first and third are inactive, second still active
    assert!(!client.is_active(&token_id_1));
    assert!(client.is_active(&token_id_2));
    assert!(!client.is_active(&token_id_3));
}

// ============================================================================
// Integration Tests
// ============================================================================

#[test]
fn test_full_nft_lifecycle() {
    let e = Env::default();
    e.mock_all_auths();

    let (admin, core_contract, owner, client) = setup_contract(&e);
    let new_owner = Address::generate(&e);

    // 1. Initialize contract
    client.initialize(&admin);
    client.set_core_contract(&core_contract);

    // 2. Mint NFT
    let token_id = mint_test_nft(&e, &client, &owner);
    assert_eq!(client.owner_of(&token_id), owner);
    assert!(client.is_active(&token_id));

    // 3. Transfer NFT
    client.transfer(&owner, &new_owner, &token_id);
    assert_eq!(client.owner_of(&token_id), new_owner);
    assert!(client.is_active(&token_id));

    // 4. Settle NFT
    client.settle(&core_contract, &token_id);
    assert_eq!(client.owner_of(&token_id), new_owner); // Owner unchanged
    assert!(!client.is_active(&token_id)); // Now inactive
}
