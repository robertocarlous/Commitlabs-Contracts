#![no_std]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, String, Symbol,
};

#[cfg(test)]
mod tests;

// ============================================================================
// Error Types
// ============================================================================

/// Contract errors for structured error handling
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    /// Contract has not been initialized
    NotInitialized = 1,
    /// Contract has already been initialized
    AlreadyInitialized = 2,
    /// NFT with the given token_id does not exist
    NFTNotFound = 3,
    /// Caller is not authorized to perform this action
    Unauthorized = 4,
    /// NFT has already been settled
    AlreadySettled = 5,
    /// Commitment has not expired yet
    NotExpired = 6,
    /// Caller is not the owner of the NFT
    NotOwner = 7,
}

// ============================================================================
// Data Types
// ============================================================================

/// Metadata associated with a commitment NFT
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitmentMetadata {
    pub commitment_id: String,
    pub duration_days: u32,
    pub max_loss_percent: u32,
    pub commitment_type: String, // "safe", "balanced", "aggressive"
    pub created_at: u64,
    pub expires_at: u64,
    pub initial_amount: i128,
    pub asset_address: Address,
}

/// The Commitment NFT structure
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitmentNFT {
    pub owner: Address,
    pub token_id: u32,
    pub metadata: CommitmentMetadata,
    pub is_active: bool,
    pub early_exit_penalty: u32,
}

/// Storage keys for the contract
#[contracttype]
pub enum DataKey {
    /// Admin address (singleton)
    Admin,
    /// Authorized commitment_core contract address
    CoreContract,
    /// Counter for generating unique token IDs
    TokenCounter,
    /// NFT data storage (token_id -> CommitmentNFT)
    NFTData(u32),
    /// Owner mapping (token_id -> Address)
    Owner(u32),
    /// Metadata storage (token_id -> CommitmentMetadata)
    Metadata(u32),
    /// Active status (token_id -> bool)
    ActiveStatus(u32),
    /// Total supply of NFTs
    TotalSupply,
}

// ============================================================================
// Storage Module
// ============================================================================

mod storage {
    use super::*;

    // --- Admin Management ---

    pub fn set_admin(e: &Env, admin: &Address) {
        e.storage().instance().set(&DataKey::Admin, admin);
    }

    pub fn get_admin(e: &Env) -> Option<Address> {
        e.storage().instance().get(&DataKey::Admin)
    }

    pub fn has_admin(e: &Env) -> bool {
        e.storage().instance().has(&DataKey::Admin)
    }

    // --- Core Contract (Access Control) ---

    pub fn set_core_contract(e: &Env, core: &Address) {
        e.storage().instance().set(&DataKey::CoreContract, core);
    }

    pub fn get_core_contract(e: &Env) -> Option<Address> {
        e.storage().instance().get(&DataKey::CoreContract)
    }

    // --- Token Counter ---

    pub fn increment_token_counter(e: &Env) -> u32 {
        let count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::TokenCounter)
            .unwrap_or(0);
        let new_count = count + 1;
        e.storage()
            .instance()
            .set(&DataKey::TokenCounter, &new_count);
        new_count
    }

    pub fn get_token_counter(e: &Env) -> u32 {
        e.storage()
            .instance()
            .get(&DataKey::TokenCounter)
            .unwrap_or(0)
    }

    // --- NFT Data ---

    pub fn set_nft_data(e: &Env, token_id: u32, nft: &CommitmentNFT) {
        e.storage()
            .persistent()
            .set(&DataKey::NFTData(token_id), nft);
    }

    pub fn get_nft_data(e: &Env, token_id: u32) -> Option<CommitmentNFT> {
        e.storage().persistent().get(&DataKey::NFTData(token_id))
    }

    // --- Owner Mapping ---

    pub fn set_owner(e: &Env, token_id: u32, owner: &Address) {
        e.storage()
            .persistent()
            .set(&DataKey::Owner(token_id), owner);
    }

    pub fn get_owner(e: &Env, token_id: u32) -> Option<Address> {
        e.storage().persistent().get(&DataKey::Owner(token_id))
    }

    // --- Metadata ---

    pub fn set_metadata(e: &Env, token_id: u32, metadata: &CommitmentMetadata) {
        e.storage()
            .persistent()
            .set(&DataKey::Metadata(token_id), metadata);
    }

    pub fn get_metadata(e: &Env, token_id: u32) -> Option<CommitmentMetadata> {
        e.storage().persistent().get(&DataKey::Metadata(token_id))
    }

    // --- Active Status ---

    pub fn set_active_status(e: &Env, token_id: u32, is_active: bool) {
        e.storage()
            .persistent()
            .set(&DataKey::ActiveStatus(token_id), &is_active);
    }

    pub fn is_active(e: &Env, token_id: u32) -> bool {
        e.storage()
            .persistent()
            .get(&DataKey::ActiveStatus(token_id))
            .unwrap_or(false)
    }

    // --- Total Supply ---

    pub fn increment_total_supply(e: &Env) {
        let supply: u32 = e
            .storage()
            .instance()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0);
        e.storage()
            .instance()
            .set(&DataKey::TotalSupply, &(supply + 1));
    }

    pub fn get_total_supply(e: &Env) -> u32 {
        e.storage()
            .instance()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0)
    }
}

// ============================================================================
// Contract Implementation
// ============================================================================

#[contract]
pub struct CommitmentNFTContract;

#[contractimpl]
impl CommitmentNFTContract {
    // ========================================================================
    // Initialization
    // ========================================================================

    /// Initialize the NFT contract with an admin address
    ///
    /// # Arguments
    /// * `admin` - The admin address that will have control over the contract
    ///
    /// # Errors
    /// * `AlreadyInitialized` - If the contract has already been initialized
    pub fn initialize(e: Env, admin: Address) -> Result<(), ContractError> {
        if storage::has_admin(&e) {
            return Err(ContractError::AlreadyInitialized);
        }

        storage::set_admin(&e, &admin);
        e.storage().instance().set(&DataKey::TokenCounter, &0u32);
        e.storage().instance().set(&DataKey::TotalSupply, &0u32);

        Ok(())
    }

    // ========================================================================
    // Access Control
    // ========================================================================

    /// Set the authorized commitment_core contract address
    /// Only the admin can call this function
    ///
    /// # Arguments
    /// * `core_contract` - The address of the commitment_core contract
    ///
    /// # Errors
    /// * `NotInitialized` - If the contract has not been initialized
    /// * `Unauthorized` - If the caller is not the admin
    pub fn set_core_contract(e: Env, core_contract: Address) -> Result<(), ContractError> {
        let admin = storage::get_admin(&e).ok_or(ContractError::NotInitialized)?;
        admin.require_auth();

        storage::set_core_contract(&e, &core_contract);

        // Emit event for access control change
        e.events()
            .publish((Symbol::new(&e, "CoreContractSet"),), (core_contract,));

        Ok(())
    }

    /// Get the authorized commitment_core contract address
    pub fn get_core_contract(e: Env) -> Result<Address, ContractError> {
        storage::get_core_contract(&e).ok_or(ContractError::NotInitialized)
    }

    /// Get the admin address
    pub fn get_admin(e: Env) -> Result<Address, ContractError> {
        storage::get_admin(&e).ok_or(ContractError::NotInitialized)
    }

    // ========================================================================
    // NFT Minting
    // ========================================================================

    /// Mint a new Commitment NFT
    ///
    /// # Arguments
    /// * `owner` - The address that will own the NFT
    /// * `commitment_id` - Unique identifier for the commitment
    /// * `duration_days` - Duration of the commitment in days
    /// * `max_loss_percent` - Maximum allowed loss percentage (0-100)
    /// * `commitment_type` - Type of commitment ("safe", "balanced", "aggressive")
    /// * `initial_amount` - Initial amount committed
    /// * `asset_address` - Address of the asset contract
    ///
    /// # Returns
    /// The token_id of the newly minted NFT
    pub fn mint(
        e: Env,
        owner: Address,
        commitment_id: String,
        duration_days: u32,
        max_loss_percent: u32,
        commitment_type: String,
        initial_amount: i128,
        asset_address: Address,
    ) -> Result<u32, ContractError> {
        // Generate unique token_id
        let token_id = storage::increment_token_counter(&e);

        // Calculate timestamps
        let created_at = e.ledger().timestamp();
        let expires_at = created_at + (duration_days as u64 * 86400);

        // Create metadata
        let metadata = CommitmentMetadata {
            commitment_id: commitment_id.clone(),
            duration_days,
            max_loss_percent,
            commitment_type,
            created_at,
            expires_at,
            initial_amount,
            asset_address,
        };

        // Create NFT
        let nft = CommitmentNFT {
            owner: owner.clone(),
            token_id,
            metadata: metadata.clone(),
            is_active: true,
            early_exit_penalty: 10, // Default 10% penalty
        };

        // Store all data
        storage::set_nft_data(&e, token_id, &nft);
        storage::set_owner(&e, token_id, &owner);
        storage::set_metadata(&e, token_id, &metadata);
        storage::set_active_status(&e, token_id, true);
        storage::increment_total_supply(&e);

        // Emit Mint event
        e.events().publish(
            (Symbol::new(&e, "Mint"), token_id),
            (owner, commitment_id, created_at),
        );

        Ok(token_id)
    }

    // ========================================================================
    // NFT Query Functions
    // ========================================================================

    /// Get NFT metadata by token_id
    pub fn get_metadata(e: Env, token_id: u32) -> Result<CommitmentMetadata, ContractError> {
        storage::get_metadata(&e, token_id).ok_or(ContractError::NFTNotFound)
    }

    /// Get owner of NFT
    pub fn owner_of(e: Env, token_id: u32) -> Result<Address, ContractError> {
        storage::get_owner(&e, token_id).ok_or(ContractError::NFTNotFound)
    }

    /// Check if NFT is active
    pub fn is_active(e: Env, token_id: u32) -> bool {
        storage::is_active(&e, token_id)
    }

    /// Get total supply of NFTs
    pub fn total_supply(e: Env) -> u32 {
        storage::get_total_supply(&e)
    }

    /// Get current token ID counter
    pub fn current_token_id(e: Env) -> u32 {
        storage::get_token_counter(&e)
    }

    /// Get full NFT data
    pub fn get_nft(e: Env, token_id: u32) -> Result<CommitmentNFT, ContractError> {
        storage::get_nft_data(&e, token_id).ok_or(ContractError::NFTNotFound)
    }

    // ========================================================================
    // NFT Transfer
    // ========================================================================

    /// Transfer NFT to new owner
    ///
    /// # Arguments
    /// * `from` - Current owner address
    /// * `to` - New owner address
    /// * `token_id` - Token ID to transfer
    ///
    /// # Errors
    /// * `NFTNotFound` - If the NFT does not exist
    /// * `NotOwner` - If the caller is not the owner
    pub fn transfer(
        e: Env,
        from: Address,
        to: Address,
        token_id: u32,
    ) -> Result<(), ContractError> {
        // Require authorization from the current owner
        from.require_auth();

        // Verify ownership
        let current_owner = storage::get_owner(&e, token_id).ok_or(ContractError::NFTNotFound)?;
        if current_owner != from {
            return Err(ContractError::NotOwner);
        }

        // Update owner in storage
        storage::set_owner(&e, token_id, &to);

        // Update NFT data to reflect new owner
        if let Some(mut nft) = storage::get_nft_data(&e, token_id) {
            nft.owner = to.clone();
            storage::set_nft_data(&e, token_id, &nft);
        }

        // Emit Transfer event
        e.events().publish(
            (Symbol::new(&e, "Transfer"), token_id),
            (from, to, e.ledger().timestamp()),
        );

        Ok(())
    }

    // ========================================================================
    // Settlement (Issue #5 - Main Implementation)
    // ========================================================================

    /// Mark NFT as settled (after maturity)
    ///
    /// This function can only be called by the authorized commitment_core contract.
    /// It marks the NFT as inactive and emits a Settle event.
    ///
    /// # Arguments
    /// * `caller` - The address of the caller (must be commitment_core contract)
    /// * `token_id` - The token ID to settle
    ///
    /// # Errors
    /// * `NotInitialized` - If the contract or core contract is not initialized
    /// * `Unauthorized` - If the caller is not the authorized core contract
    /// * `NFTNotFound` - If the NFT does not exist
    /// * `AlreadySettled` - If the NFT has already been settled
    ///
    /// # Events
    /// Emits a `Settle` event with:
    /// - token_id
    /// - timestamp
    /// - final_status ("settled")
    pub fn settle(e: Env, caller: Address, token_id: u32) -> Result<(), ContractError> {
        // 1. Access Control: Verify caller signed this transaction
        caller.require_auth();

        // 2. Access Control: Only commitment_core contract can call this
        let core_contract = storage::get_core_contract(&e).ok_or(ContractError::NotInitialized)?;
        if caller != core_contract {
            return Err(ContractError::Unauthorized);
        }

        // 2. Verify NFT exists
        let mut nft = storage::get_nft_data(&e, token_id).ok_or(ContractError::NFTNotFound)?;

        // 3. Check if already settled
        if !nft.is_active {
            return Err(ContractError::AlreadySettled);
        }

        // 4. Check if commitment has expired (optional - may be handled by core contract)
        // The issue states this is optional since core contract may handle it
        // Uncomment if NFT contract should also verify expiration:
        // let now = e.ledger().timestamp();
        // if now < nft.metadata.expires_at {
        //     return Err(ContractError::NotExpired);
        // }

        // 5. Mark NFT as inactive in storage
        nft.is_active = false;
        storage::set_nft_data(&e, token_id, &nft);
        storage::set_active_status(&e, token_id, false);

        // 6. Emit Settle event with: token_id, timestamp, final_status
        let timestamp = e.ledger().timestamp();
        let final_status = String::from_str(&e, "settled");
        e.events().publish(
            (Symbol::new(&e, "Settle"), token_id),
            (timestamp, final_status),
        );

        Ok(())
    }
}
