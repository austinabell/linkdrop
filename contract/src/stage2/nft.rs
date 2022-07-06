use crate::*;

/// Keep track of nft data. This is stored on the contract
#[derive(BorshDeserialize, BorshSerialize)]
pub struct NFTData {
    pub nft_sender: AccountId,
    pub nft_contract: AccountId,
    pub longest_token_id: String,
    pub storage_for_longest: Balance,
    pub token_ids: UnorderedSet<String>,
}

/// Keep track of nft data. This is passed in by the user
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct NFTDataConfig {
    pub nft_sender: AccountId,
    pub nft_contract: AccountId,
    pub longest_token_id: String,
}

#[near_bindgen]
impl DropZone {
    pub fn nft_on_transfer(
        &mut self,
        token_id: String,
        sender_id: AccountId,
        msg: U128,
    ) -> PromiseOrValue<bool> {
        let contract_id = env::predecessor_account_id();

        let mut drop = self.drop_for_id.get(&msg.0).expect("No drop found for ID");
        let DropType::NFT(mut nft_data) = drop.drop_type;
        let mut token_ids = nft_data.token_ids;

        require!(nft_data.nft_sender == sender_id && nft_data.nft_contract == contract_id, "NFT data must match what was sent");
        require!(token_id.len() <= nft_data.longest_token_id.len(), "token ID must be less than largest token specified");
    
        require!(token_ids.insert(&token_id) == true, "token ID already registered");

        // Re-insert the token IDs into the NFT Data struct 
        nft_data.token_ids = token_ids;

        // Increment the claims registered
        drop.num_claims_registered += 1;
        env::log_str(&format!("drop.num_claims_registered {}", drop.num_claims_registered));

        // Ensure that the keys to register can't exceed the number of keys in the drop.
        if drop.num_claims_registered > drop.pks.len() * drop.drop_config.max_claims_per_key {
            env::log_str("Too many NFTs sent. Contract is keeping the rest.");
            drop.num_claims_registered = drop.pks.len() * drop.drop_config.max_claims_per_key;
        }

        // Add the nft data back with the updated set
        drop.drop_type = DropType::NFT(nft_data);

        // Insert the drop with the updated data
        self.drop_for_id.insert(&msg.0, &drop);

        // Everything went well and we don't need to return the token.
        PromiseOrValue::Value(false)
    }

    #[private]
    /// self callback checks if NFT was successfully transferred to the new account. If yes, do nothing. If no, refund original sender
    pub fn nft_resolve_refund(
        &mut self, 
        drop_id: U128,
        token_ids: Vec<String>, 
    ) -> bool {
        let used_gas = env::used_gas();
        let prepaid_gas = env::prepaid_gas();

        env::log_str(&format!("Beginning of resolve refund used gas: {:?} prepaid gas: {:?}", used_gas.0 / ONE_GIGGA_GAS, prepaid_gas.0 / ONE_GIGGA_GAS));
        let transfer_succeeded = matches!(env::promise_result(0), PromiseResult::Successful(_));
        
        // If not successful, the length of the token IDs needs to be added back to the drop.
        if !transfer_succeeded {
            let mut drop = self.drop_for_id.get(&drop_id.0).unwrap();
            drop.num_claims_registered += token_ids.len() as u64;
            self.drop_for_id.insert(&drop_id.0, &drop);

            env::log_str(&format!("Transfer failed. Adding {} back to drop's keys registered", token_ids.len() as u64));

            return false
        }

        // Loop through and remove each token ID from the drop's NFT data token IDs
        let mut drop = self.drop_for_id.get(&drop_id.0).unwrap();
        let DropType::NFT(mut nft_data) = drop.drop_type;
        let mut ids = nft_data.token_ids;

        for id in token_ids {
            env::log_str(&format!("Removing {}. Present: {}", id, ids.remove(&id)));
        }

        nft_data.token_ids = ids;
        drop.drop_type = DropType::NFT(nft_data);

        return true
    }

    #[private]
    /// self callback checks if NFT was successfully transferred to the new account. If yes, do nothing. If no, refund original sender
    pub fn nft_resolve_transfer(
        &mut self, 
        token_id: String, 
        token_sender: AccountId,
        token_contract: AccountId 
    ) -> bool {
        let mut used_gas = env::used_gas();
        let mut prepaid_gas = env::prepaid_gas();

        env::log_str(&format!("Beginning of resolve transfer used gas: {:?} prepaid gas: {:?}", used_gas.0 / ONE_GIGGA_GAS, prepaid_gas.0 / ONE_GIGGA_GAS));
        let transfer_succeeded = matches!(env::promise_result(0), PromiseResult::Successful(_));

        used_gas = env::used_gas();
        prepaid_gas = env::prepaid_gas();
        env::log_str(&format!("Before refunding token sender in resolve transfer: {:?} prepaid gas: {:?}", used_gas.0 / ONE_GIGGA_GAS, prepaid_gas.0 / ONE_GIGGA_GAS));

        // If not successful, the balance is added to the amount to refund since it was never transferred.
        if !transfer_succeeded {
            env::log_str("Attempt to transfer the new account was unsuccessful. Sending the NFT to the original sender.");
            ext_nft_contract::ext(token_contract)
                // Call nft transfer with the min GAS and 1 yoctoNEAR. all unspent GAS will be added on top
                .with_static_gas(MIN_GAS_FOR_SIMPLE_NFT_TRANSFER)
                .with_attached_deposit(1)
                .nft_transfer(
                    token_sender, 
                    token_id,
                    None,
                    Some("Linkdropped NFT Refund".to_string()),
                );
        }

        transfer_succeeded
    }
}
