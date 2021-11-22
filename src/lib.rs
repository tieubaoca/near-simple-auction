use near_contract_standards::non_fungible_token::core::NonFungibleTokenCore;
use near_contract_standards::non_fungible_token::metadata::TokenMetadata;
use near_contract_standards::non_fungible_token::{NonFungibleToken, Token, TokenId};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::*;
use near_sdk::serde::{Deserialize, Serialize};
const MINT_FEE: Balance = 1_000_000_000_000_000_000_000_00;
const CREATE_AUCTION_FEE: Balance = 1_000_000_000_000_000_000_000_000;
const ENROLL_FEE: Balance = 1_000_000_000_000_000_000_000_00;
use near_sdk::json_types::ValidAccountId;
use near_sdk::{
    env, ext_contract, near_bindgen, AccountId, Balance, BorshStorageKey, PanicOnDefault, Promise,
    PromiseOrValue,
};
#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct NFTMarket {
    owner: AccountId,
    tokens: NonFungibleToken,
    total_auctions: u128,
    auction_by_id: UnorderedMap<u128, Auction>,
    auctions_by_owner: UnorderedMap<AccountId, Vector<u128>>,
    auctioned_tokens: UnorderedSet<TokenId>,
}

//near_contract_standards::impl_non_fungible_token_core!(NFTMarket, tokens);
near_contract_standards::impl_non_fungible_token_approval!(NFTMarket, tokens);
near_contract_standards::impl_non_fungible_token_enumeration!(NFTMarket, tokens);

#[near_bindgen]
impl NFTMarket {
    #[init]
    pub fn new() -> Self {
        assert!(!env::state_exists(), "Already initialized");
        Self {
            owner: env::predecessor_account_id(),
            tokens: NonFungibleToken::new(
                StorageKey::NonFungibleToken,
                ValidAccountId::try_from(env::predecessor_account_id()).unwrap(),
                Some(StorageKey::TokenMetadata),
                Some(StorageKey::Enumeration),
                Some(StorageKey::Approval),
            ),
            total_auctions: 0,
            auction_by_id: UnorderedMap::new(b"auction_by_id".to_vec()), //
            auctions_by_owner: UnorderedMap::new(b"auctions_by_owner".to_vec()),
            auctioned_tokens: UnorderedSet::new(b"is_token_auctioned".to_vec()),
        }
    }
    #[payable]
    pub fn mint(
        &mut self,
        token_id: TokenId,
        token_owner_id: ValidAccountId,
        token_metadata: Option<TokenMetadata>,
    ) -> Token {
        assert_eq!(
            env::attached_deposit(),
            MINT_FEE,
            "Require 0.1N to mint NFT"
        );
        self.tokens.mint(token_id, token_owner_id, token_metadata)
    }

    #[payable]
    pub fn nft_transfer(
        &mut self,
        receiver_id: ValidAccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        memo: Option<String>,
    ) {
        self.tokens
            .nft_transfer(receiver_id, token_id, approval_id, memo)
    }

    #[payable]
    pub fn nft_transfer_call(
        &mut self,
        receiver_id: ValidAccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<bool> {
        self.tokens
            .nft_transfer_call(receiver_id, token_id, approval_id, memo, msg)
    }

    pub fn nft_token(self, token_id: TokenId) -> Option<Token> {
        self.tokens.nft_token(token_id)
    }

    #[payable]
    pub fn create_auction(
        &mut self,
        auction_token: TokenId,
        start_price: Balance,
        start_time: u64,
        end_time: u64,
    ) -> Auction {
        let owner_id = self.tokens.owner_by_id.get(&auction_token).unwrap();
        assert_eq!(
            owner_id,
            env::predecessor_account_id(),
            "You not own this NFT"
        );
        assert_eq!(
            self.auctioned_tokens.contains(&auction_token),
            false,
            "Already auctioned"
        );
        assert_eq!(
            env::attached_deposit(),
            CREATE_AUCTION_FEE,
            "Require 1N to create an auction"
        );
        self.tokens.internal_transfer(
            &env::predecessor_account_id(),
            &env::current_account_id(),
            &auction_token,
            None,
            None,
        );
        let mut auction_ids: Vector<u128>;
        if self
            .auctions_by_owner
            .get(&env::predecessor_account_id())
            .is_none()
        {
            auction_ids = Vector::new(b"auction_ids".to_vec());
        } else {
            auction_ids = self
                .auctions_by_owner
                .get(&env::predecessor_account_id())
                .unwrap();
        }
        auction_ids.push(&self.total_auctions);
        let auction = Auction {
            owner: owner_id,
            auction_id: self.total_auctions,
            auction_token: auction_token.clone(),
            start_price,
            start_time: start_time * 1_000_000_000,
            end_time: end_time * 1_000_000_000,
            current_price: start_price,
            winner: String::new(),
            is_near_claimed: false,
            is_nft_claimed: false,
        };
        self.auctions_by_owner
            .insert(&env::predecessor_account_id(), &auction_ids);
        self.auction_by_id.insert(&self.total_auctions, &auction);
        self.auctioned_tokens.insert(&auction_token);
        self.total_auctions += 1;
        auction
    }

    #[payable]
    pub fn bid(&mut self, auction_id: u128) {
        let mut auction = self.auction_by_id.get(&auction_id).unwrap_or_else(|| {
            panic!("This auction does not exist");
        });
        assert_eq!(
            env::block_timestamp() > auction.start_time,
            true,
            "This auction has not started"
        );
        assert_eq!(
            env::block_timestamp() < auction.end_time,
            true,
            "This auction has already done"
        );
        assert_eq!(
            env::attached_deposit() > auction.current_price,
            true,
            "Price must be greater than current winner's price"
        );
        if !(auction.winner == String::new()) {
            let old_winner = Promise::new(auction.winner);
            old_winner.transfer(auction.current_price - ENROLL_FEE);
        }
        auction.winner = env::predecessor_account_id();
        auction.current_price = env::attached_deposit();
        self.auction_by_id.insert(&auction_id, &auction);
    }

    #[payable]
    pub fn claim_nft(&mut self, auction_id: u128) {
        let mut auction = self.auction_by_id.get(&auction_id).unwrap_or_else(|| {
            panic!("This auction does not exist");
        });
        assert_eq!(
            env::block_timestamp() > auction.end_time,
            true,
            "The auction is not over yet"
        );
        assert_eq!(
            env::predecessor_account_id(),
            auction.winner,
            "You are not the winner"
        );
        assert_eq!(
            auction.clone().is_nft_claimed,
            false,
            "You has already claimed NFT"
        );
        self.tokens.internal_transfer_unguarded(
            &auction.auction_token,
            &env::current_account_id(),
            &auction.winner,
        );
        auction.is_nft_claimed = true;
        self.auctioned_tokens.remove(&auction.auction_token);
        self.auction_by_id.insert(&auction_id, &auction);
    }

    #[payable]
    pub fn claim_near(&mut self, auction_id: u128) {
        let mut auction = self.auction_by_id.get(&auction_id).unwrap_or_else(|| {
            panic!("This auction does not exist");
        });
        assert_eq!(
            env::predecessor_account_id(),
            auction.owner,
            "You are not operator of this auction"
        );
        assert_eq!(
            env::block_timestamp() > auction.end_time,
            true,
            "The auction is not over yet"
        );
        assert_eq!(auction.is_near_claimed, false, "You has already claimed N");
        Promise::new(auction.clone().owner).transfer(auction.current_price);
        auction.is_near_claimed = true;
        self.auction_by_id.insert(&auction_id, &auction);
    }

    #[payable]
    pub fn claim_back_nft(&mut self, auction_id: u128) {
        let mut auction = self.auction_by_id.get(&auction_id).unwrap_or_else(|| {
            panic!("This auction does not exist");
        });
        assert_eq!(
            env::predecessor_account_id(),
            auction.owner,
            "You are not operator of this auction"
        );
        assert_eq!(
            env::block_timestamp() > auction.end_time,
            true,
            "The auction is not over yet"
        );
        assert_eq!(auction.winner, String::new(), "The NFT has sold");
        self.tokens.internal_transfer_unguarded(
            &auction.auction_token,
            &env::current_account_id(),
            &auction.owner,
        );
        auction.is_nft_claimed = true;
        self.auctioned_tokens.remove(&auction.auction_token);
        self.auction_by_id.insert(&auction_id, &auction);
    }

    pub fn get_auction(&self, auction_id: u128) -> Auction {
        self.auction_by_id.get(&auction_id).unwrap()
    }
}

#[derive(Debug, BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct Auction {
    owner: AccountId,
    auction_id: u128,
    auction_token: TokenId,
    start_price: Balance,
    start_time: u64,
    end_time: u64,
    current_price: Balance,
    winner: AccountId,
    is_near_claimed: bool,
    is_nft_claimed: bool,
}

#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
    NonFungibleToken,
    TokenMetadata,
    Enumeration,
    Approval,
}

#[ext_contract(ex_self)]
trait MyContract {
    fn external_mint(
        &mut self,
        token_id: TokenId,
        token_owner_id: ValidAccountId,
        token_metadata: Option<TokenMetadata>,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_contract_standards::non_fungible_token::metadata::TokenMetadata;
    use near_sdk::json_types::ValidAccountId;
    use near_sdk::MockedBlockchain;
    use near_sdk::{testing_env, VMContext};
    use near_sdk::{AccountId, Balance};

    fn bob() -> ValidAccountId {
        ValidAccountId::try_from("bob.testnet").unwrap()
    }
    fn senna() -> ValidAccountId {
        ValidAccountId::try_from("senna.testnet").unwrap()
    }
    fn alice() -> ValidAccountId {
        ValidAccountId::try_from("alice.testnet").unwrap()
    }
    fn carol() -> ValidAccountId {
        ValidAccountId::try_from("carol.testnet").unwrap()
    }
    fn smith() -> ValidAccountId {
        ValidAccountId::try_from("smith.testnet").unwrap()
    }
    fn john() -> ValidAccountId {
        ValidAccountId::try_from("john.testnet").unwrap()
    }
    fn lili() -> ValidAccountId {
        ValidAccountId::try_from("ili.testnet").unwrap()
    }
    fn james() -> ValidAccountId {
        ValidAccountId::try_from("james.testnet").unwrap()
    }
    fn nft(title: &str) -> TokenMetadata {
        TokenMetadata {
            title: Some(String::from(title)), // ex. "Arch Nemesis: Mail Carrier" or "Parcel #5055"
            description: Some(String::from(title)), // free-form description
            media: None, // URL to associated media, preferably to decentralized, content-addressed storage
            media_hash: None, // Base64-encoded sha256 hash of content referenced by the `media` field. Required if `media` is included.
            copies: None, // number of copies of this set of metadata in existence when token was minted.
            issued_at: None, // ISO 8601 datetime when token was issued or minted
            expires_at: None, // ISO 8601 datetime when token expires
            starts_at: None, // ISO 8601 datetime when token starts being valid
            updated_at: None, // ISO 8601 datetime when token was last updated
            extra: None, // anything extra the NFT wants to store on-chain. Can be stringified JSON.
            reference: None, // URL to an off-chain JSON file with more info.
            reference_hash: None, // Base64-encoded sha256 hash of JSON from reference field. Required if `reference` is included.
        }
    }

    // part of writing unit tests is setting up a mock context
    // this is a useful list to peek at when wondering what's available in env::*
    fn get_context(
        account_id: String,
        current_account_id: String,
        storage_usage: u64,
        block_timestamp: u64,
        attached_deposit: Balance,
    ) -> VMContext {
        VMContext {
            current_account_id,
            signer_account_id: account_id.clone(),
            signer_account_pk: vec![0, 1, 2],
            predecessor_account_id: account_id,
            input: vec![],
            block_index: 0,
            block_timestamp,
            account_balance: 1_00_000_000_000_000_000_000_000_000,
            account_locked_balance: 0,
            storage_usage,
            attached_deposit,
            prepaid_gas: 10u64.pow(18),
            random_seed: vec![0, 1, 2],
            is_view: false,
            output_data_receivers: vec![],
            epoch_height: 19,
        }
    }

    #[test]
    fn test_mint_nft() {
        let context = get_context(senna().into(), senna().into(), 0, 0, 0);
        testing_env!(context);
        let mut contract = NFTMarket::new();
        testing_env!(get_context(senna().into(), senna().into(), 0, 0, MINT_FEE));
        contract.mint(String::from("1"), bob(), Some(nft("first")));
        assert_eq!(
            contract.nft_token(String::from("1")).unwrap().owner_id,
            bob().to_string(),
            "Owner must be bob"
        );
    }

    #[test]
    #[should_panic(expected = "You not own this NFT")]
    fn test_auction_panic_not_own_nft() {
        let context = get_context(senna().into(), senna().into(), 0, 0, 0);
        testing_env!(context);
        let mut contract = NFTMarket::new();
        testing_env!(get_context(senna().into(), senna().into(), 0, 0, MINT_FEE));
        contract.mint(String::from("1"), bob(), Some(nft("first")));
        testing_env!(get_context(
            senna().into(),
            senna().into(),
            0,
            0,
            CREATE_AUCTION_FEE
        ));
        contract.create_auction(
            String::from("1"),
            1_000_000_000_000_000_000_000_000,
            100,
            3700,
        );
        //testing_env!(get_context(alice().into(), 0,0,1_500_000_000_000_000_000_000_000));
    }

    #[test]
    //#[should_panic(expected = "This auction has not been started or already done")]
    fn test_auction_panic_bid_too_early() {
        let context = get_context(senna().into(), senna().into(), 0, 0, 0);
        testing_env!(context);
        let mut contract = NFTMarket::new();
        testing_env!(get_context(senna().into(), senna().into(), 0, 0, MINT_FEE));
        contract.mint(String::from("1"), alice(), Some(nft("first")));
        testing_env!(get_context(
            alice().into(),
            senna().into(),
            0,
            0,
            CREATE_AUCTION_FEE
        ));
        contract.create_auction(String::from("1"), 1_000_000_000, 1000, 4600);
        testing_env!(get_context(
            bob().into(),
            senna().into(),
            0,
            50,
            1_500_000_000_000_000_000_000_000
        ));
        //contract.bid(0);
    }
}
