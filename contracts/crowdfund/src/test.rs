use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger},
    token, Address, Env, Vec,
};

use crate::{ContractError, CrowdfundContract, CrowdfundContractClient};

#[derive(Clone)]
#[contracttype]
struct MintRecord {
    to: Address,
    token_id: u128,
}

#[contract]
struct MockNftContract;

#[contractimpl]
impl MockNftContract {
    pub fn mint(env: Env, to: Address) -> u128 {
        let next_id: u128 = env.storage().instance().get(&1u32).unwrap_or(0u128) + 1;
        env.storage().instance().set(&1u32, &next_id);

        let mut records: Vec<MintRecord> = env
            .storage()
            .persistent()
            .get(&2u32)
            .unwrap_or_else(|| Vec::new(&env));
        records.push_back(MintRecord {
            to,
            token_id: next_id,
        });
        env.storage().persistent().set(&2u32, &records);

        next_id
    }

    pub fn minted(env: Env) -> Vec<MintRecord> {
        env.storage()
            .persistent()
            .get(&2u32)
            .unwrap_or_else(|| Vec::new(&env))
    }
}

fn setup_env() -> (
    Env,
    CrowdfundContractClient<'static>,
    Address,
    Address,
    token::StellarAssetClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(CrowdfundContract, ());
    let client = CrowdfundContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token_contract_id = env.register_stellar_asset_contract_v2(token_admin);
    let token_address = token_contract_id.address();
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    let creator = Address::generate(&env);
    token_admin_client.mint(&creator, &10_000_000);

    (env, client, creator, token_address, token_admin_client)
}

#[test]
fn test_withdraw_mints_nft_for_each_contributor() {
    let (env, client, creator, token_address, token_admin_client) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;

    client.initialize(
        &creator,
        &token_address,
        &goal,
        &deadline,
        &min_contribution,
        &None,
    );

    let nft_id = env.register(MockNftContract, ());
    let nft_client = MockNftContractClient::new(&env, &nft_id);
    client.set_nft_contract(&creator, &nft_id);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    token_admin_client.mint(&alice, &600_000);
    token_admin_client.mint(&bob, &400_000);

    client.contribute(&alice, &600_000);
    client.contribute(&bob, &400_000);

    env.ledger().set_timestamp(deadline + 1);
    client.withdraw();

    let minted = nft_client.minted();
    assert_eq!(minted.len(), 2);
    assert_eq!(minted.get(0).unwrap().to, alice);
    assert_eq!(minted.get(0).unwrap().token_id, 1);
    assert_eq!(minted.get(1).unwrap().to, bob);
    assert_eq!(minted.get(1).unwrap().token_id, 2);
}

#[test]
fn test_withdraw_skips_nft_mint_when_contract_not_set() {
    let (env, client, creator, token_address, token_admin_client) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;

    client.initialize(
        &creator,
        &token_address,
        &goal,
        &deadline,
        &min_contribution,
        &None,
    );

    let nft_id = env.register(MockNftContract, ());
    let nft_client = MockNftContractClient::new(&env, &nft_id);

    let contributor = Address::generate(&env);
    token_admin_client.mint(&contributor, &goal);
    client.contribute(&contributor, &goal);

    env.ledger().set_timestamp(deadline + 1);
    client.withdraw();

    assert_eq!(nft_client.minted().len(), 0);
}

#[test]
fn test_set_nft_contract_rejects_non_creator() {
    let (env, client, creator, token_address, _token_admin_client) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;

    client.initialize(
        &creator,
        &token_address,
        &goal,
        &deadline,
        &min_contribution,
        &None,
    );

    let non_creator = Address::generate(&env);
    let nft_id = env.register(MockNftContract, ());

    let result = client.try_set_nft_contract(&non_creator, &nft_id);
    assert!(result.is_err());
}

#[test]
fn test_withdraw_successful_campaign_updates_status_and_balance() {
    let (env, client, creator, token_address, token_admin_client) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 500_000;
    let min_contribution: i128 = 1_000;

    client.initialize(
        &creator,
        &token_address,
        &goal,
        &deadline,
        &min_contribution,
        &None,
    );

    let contributor = Address::generate(&env);
    token_admin_client.mint(&contributor, &goal);
    client.contribute(&contributor, &goal);

    let token_client = token::Client::new(&env, &token_address);
    let creator_before = token_client.balance(&creator);

    env.ledger().set_timestamp(deadline + 1);
    client.withdraw();

    assert_eq!(client.total_raised(), 0);
    assert_eq!(token_client.balance(&creator), creator_before + goal);
}

#[test]
fn test_contribute_after_deadline_returns_error() {
    let (env, client, creator, token_address, token_admin_client) = setup_env();

    let deadline = env.ledger().timestamp() + 100;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;

    client.initialize(
        &creator,
        &token_address,
        &goal,
        &deadline,
        &min_contribution,
        &None,
    );

    env.ledger().set_timestamp(deadline + 1);

    let contributor = Address::generate(&env);
    token_admin_client.mint(&contributor, &500_000);

    let result = client.try_contribute(&contributor, &500_000);
    assert_eq!(result.unwrap_err().unwrap(), ContractError::CampaignEnded);
}
