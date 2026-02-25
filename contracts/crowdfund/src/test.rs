#![cfg(test)]

use soroban_sdk::{testutils::{Address as _, Ledger}, token, Address, Env};

use crate::{CrowdfundContract, CrowdfundContractClient};

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Set up a fresh environment with a deployed crowdfund contract and a token.
fn setup_env() -> (Env, CrowdfundContractClient<'static>, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    // Deploy the crowdfund contract.
    let contract_id = env.register(CrowdfundContract, ());
    let client = CrowdfundContractClient::new(&env, &contract_id);

    // Create a token for contributions.
    let token_admin = Address::generate(&env);
    let token_contract_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_address = token_contract_id.address();
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    // Platform admin and campaign creator.
    let platform_admin = Address::generate(&env);
    let creator = Address::generate(&env);

    // Mint tokens to the creator so the contract has something to work with.
    token_admin_client.mint(&creator, &10_000_000);

    (env, client, platform_admin, creator, token_address, token_admin.clone())
}

/// Helper to mint tokens to an arbitrary contributor.
fn mint_to(env: &Env, token_address: &Address, admin: &Address, to: &Address, amount: i128) {
    let admin_client = token::StellarAssetClient::new(env, token_address);
    admin_client.mint(to, &amount);
    let _ = admin;
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[test]
fn test_initialize() {
    let (env, client, platform_admin, creator, token_address, _token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600; // 1 hour from now
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;

    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    assert_eq!(client.goal(), goal);
    assert_eq!(client.deadline(), deadline);
    assert_eq!(client.min_contribution(), min_contribution);
    assert_eq!(client.total_raised(), 0);
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_double_initialize_panics() {
    let (env, client, platform_admin, creator, token_address, _token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;

    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution); // should panic
}

#[test]
fn test_contribute() {
    let (env, client, platform_admin, creator, token_address, token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    let contributor = Address::generate(&env);
    mint_to(&env, &token_address, &token_admin, &contributor, 500_000);

    client.contribute(&contributor, &500_000);

    assert_eq!(client.total_raised(), 500_000);
    assert_eq!(client.contribution(&contributor), 500_000);
}

#[test]
fn test_multiple_contributions() {
    let (env, client, platform_admin, creator, token_address, token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    mint_to(&env, &token_address, &token_admin, &alice, 600_000);
    mint_to(&env, &token_address, &token_admin, &bob, 400_000);

    client.contribute(&alice, &600_000);
    client.contribute(&bob, &400_000);

    assert_eq!(client.total_raised(), 1_000_000);
    assert_eq!(client.contribution(&alice), 600_000);
    assert_eq!(client.contribution(&bob), 400_000);
}

#[test]
#[should_panic(expected = "campaign has ended")]
fn test_contribute_after_deadline_panics() {
    let (env, client, platform_admin, creator, token_address, token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 100;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    // Fast-forward past the deadline.
    env.ledger().set_timestamp(deadline + 1);

    let contributor = Address::generate(&env);
    mint_to(&env, &token_address, &token_admin, &contributor, 500_000);

    client.contribute(&contributor, &500_000); // should panic
}

#[test]
fn test_withdraw_after_goal_met() {
    let (env, client, platform_admin, creator, token_address, token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    let contributor = Address::generate(&env);
    mint_to(&env, &token_address, &token_admin, &contributor, 1_000_000);
    client.contribute(&contributor, &1_000_000);

    assert_eq!(client.total_raised(), goal);

    // Move past deadline.
    env.ledger().set_timestamp(deadline + 1);

    client.withdraw();

    // After withdrawal, total_raised resets to 0.
    assert_eq!(client.total_raised(), 0);

    // Creator should have received the funds.
    let token_client = token::Client::new(&env, &token_address);
    assert_eq!(token_client.balance(&creator), 10_000_000 + 1_000_000);
}

#[test]
#[should_panic(expected = "campaign is still active")]
fn test_withdraw_before_deadline_panics() {
    let (env, client, platform_admin, creator, token_address, token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    let contributor = Address::generate(&env);
    mint_to(&env, &token_address, &token_admin, &contributor, 1_000_000);
    client.contribute(&contributor, &1_000_000);

    client.withdraw(); // should panic — deadline not passed
}

#[test]
#[should_panic(expected = "goal not reached")]
fn test_withdraw_goal_not_reached_panics() {
    let (env, client, platform_admin, creator, token_address, token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    let contributor = Address::generate(&env);
    mint_to(&env, &token_address, &token_admin, &contributor, 500_000);
    client.contribute(&contributor, &500_000);

    // Move past deadline, but goal not met.
    env.ledger().set_timestamp(deadline + 1);

    client.withdraw(); // should panic
}

#[test]
fn test_refund_when_goal_not_met() {
    let (env, client, platform_admin, creator, token_address, token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    mint_to(&env, &token_address, &token_admin, &alice, 300_000);
    mint_to(&env, &token_address, &token_admin, &bob, 200_000);

    client.contribute(&alice, &300_000);
    client.contribute(&bob, &200_000);

    // Move past deadline — goal not met.
    env.ledger().set_timestamp(deadline + 1);

    client.refund();

    // Both contributors should get their tokens back.
    let token_client = token::Client::new(&env, &token_address);
    assert_eq!(token_client.balance(&alice), 300_000);
    assert_eq!(token_client.balance(&bob), 200_000);
    assert_eq!(client.total_raised(), 0);
}

#[test]
#[should_panic(expected = "goal was reached; use withdraw instead")]
fn test_refund_when_goal_reached_panics() {
    let (env, client, platform_admin, creator, token_address, token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    let contributor = Address::generate(&env);
    mint_to(&env, &token_address, &token_admin, &contributor, 1_000_000);
    client.contribute(&contributor, &1_000_000);

    env.ledger().set_timestamp(deadline + 1);

    client.refund(); // should panic — goal was met
}

#[test]
#[should_panic(expected = "campaign is not active")]
fn test_double_withdraw_panics() {
    let (env, client, platform_admin, creator, token_address, token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    let contributor = Address::generate(&env);
    mint_to(&env, &token_address, &token_admin, &contributor, 1_000_000);
    client.contribute(&contributor, &1_000_000);

    env.ledger().set_timestamp(deadline + 1);

    client.withdraw();
    client.withdraw(); // should panic — status is Successful
}

#[test]
#[should_panic(expected = "campaign is not active")]
fn test_double_refund_panics() {
    let (env, client, platform_admin, creator, token_address, token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    let alice = Address::generate(&env);
    mint_to(&env, &token_address, &token_admin, &alice, 500_000);
    client.contribute(&alice, &500_000);

    env.ledger().set_timestamp(deadline + 1);

    client.refund();
    client.refund(); // should panic — status is Refunded
}

#[test]
fn test_cancel_with_no_contributions() {
    let (env, client, platform_admin, creator, token_address, _token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    client.cancel();

    assert_eq!(client.total_raised(), 0);
}

#[test]
fn test_cancel_with_contributions() {
    let (env, client, platform_admin, creator, token_address, token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    mint_to(&env, &token_address, &token_admin, &alice, 300_000);
    mint_to(&env, &token_address, &token_admin, &bob, 200_000);

    client.contribute(&alice, &300_000);
    client.contribute(&bob, &200_000);

    client.cancel();

    let token_client = token::Client::new(&env, &token_address);
    assert_eq!(token_client.balance(&alice), 300_000);
    assert_eq!(token_client.balance(&bob), 200_000);
    assert_eq!(client.total_raised(), 0);
}

#[test]
#[should_panic]
fn test_cancel_by_non_creator_panics() {
    let env = Env::default();
    let contract_id = env.register(CrowdfundContract, ());
    let client = CrowdfundContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token_contract_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_address = token_contract_id.address();

    let platform_admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let non_creator = Address::generate(&env);

    env.mock_all_auths();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    env.mock_all_auths_allowing_non_root_auth();
    env.set_auths(&[]);
    
    client.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &non_creator,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "cancel",
            args: soroban_sdk::vec![&env],
            sub_invokes: &[],
        },
    }]);

    client.cancel();
}

// ── Minimum Contribution Tests ─────────────────────────────────────────────

#[test]
#[should_panic(expected = "amount below minimum")]
fn test_contribute_below_minimum_panics() {
    let (env, client, platform_admin, creator, token_address, token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 10_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    let contributor = Address::generate(&env);
    mint_to(&env, &token_address, &token_admin, &contributor, 5_000);

    client.contribute(&contributor, &5_000); // should panic
}

#[test]
fn test_contribute_exact_minimum() {
    let (env, client, platform_admin, creator, token_address, token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 10_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    let contributor = Address::generate(&env);
    mint_to(&env, &token_address, &token_admin, &contributor, 10_000);

    client.contribute(&contributor, &10_000);

    assert_eq!(client.total_raised(), 10_000);
    assert_eq!(client.contribution(&contributor), 10_000);
}

#[test]
fn test_contribute_above_minimum() {
    let (env, client, platform_admin, creator, token_address, token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 10_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    let contributor = Address::generate(&env);
    mint_to(&env, &token_address, &token_admin, &contributor, 50_000);

    client.contribute(&contributor, &50_000);

    assert_eq!(client.total_raised(), 50_000);
    assert_eq!(client.contribution(&contributor), 50_000);
}

#[test]
fn test_token_address_view() {
    let (env, client, platform_admin, creator, token_address, _token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;

    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    assert_eq!(client.token(), token_address);
}

// ── Contributors List Tests ────────────────────────────────────────────────

#[test]
fn test_contributors_empty_list() {
    let (env, client, platform_admin, creator, token_address, _token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    let contributors = client.contributors();
    assert_eq!(contributors.len(), 0);
}

#[test]
fn test_contributors_single_contributor() {
    let (env, client, platform_admin, creator, token_address, token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    let alice = Address::generate(&env);
    mint_to(&env, &token_address, &token_admin, &alice, 500_000);
    client.contribute(&alice, &500_000);

    let contributors = client.contributors();
    assert_eq!(contributors.len(), 1);
    assert_eq!(contributors.get(0).unwrap(), alice);
}

#[test]
fn test_contributors_multiple_contributors() {
    let (env, client, platform_admin, creator, token_address, token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);
    
    mint_to(&env, &token_address, &token_admin, &alice, 300_000);
    mint_to(&env, &token_address, &token_admin, &bob, 400_000);
    mint_to(&env, &token_address, &token_admin, &charlie, 300_000);

    client.contribute(&alice, &300_000);
    client.contribute(&bob, &400_000);
    client.contribute(&charlie, &300_000);

    let contributors = client.contributors();
    assert_eq!(contributors.len(), 3);
    assert!(contributors.contains(&alice));
    assert!(contributors.contains(&bob));
    assert!(contributors.contains(&charlie));
}

#[test]
fn test_contributors_duplicate_contributions() {
    let (env, client, platform_admin, creator, token_address, token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    let alice = Address::generate(&env);
    mint_to(&env, &token_address, &token_admin, &alice, 600_000);

    // Alice contributes multiple times
    client.contribute(&alice, &300_000);
    client.contribute(&alice, &300_000);

    let contributors = client.contributors();
    // Should only appear once in the list
    assert_eq!(contributors.len(), 1);
    assert_eq!(contributors.get(0).unwrap(), alice);
}

#[test]
fn test_contributors_order_preserved() {
    let (env, client, platform_admin, creator, token_address, token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);
    
    mint_to(&env, &token_address, &token_admin, &alice, 100_000);
    mint_to(&env, &token_address, &token_admin, &bob, 100_000);
    mint_to(&env, &token_address, &token_admin, &charlie, 100_000);

    // Contribute in specific order
    client.contribute(&alice, &100_000);
    client.contribute(&bob, &100_000);
    client.contribute(&charlie, &100_000);

    let contributors = client.contributors();
    assert_eq!(contributors.len(), 3);
    // Verify order is preserved
    assert_eq!(contributors.get(0).unwrap(), alice);
    assert_eq!(contributors.get(1).unwrap(), bob);
    assert_eq!(contributors.get(2).unwrap(), charlie);
}

// ── Verified Creator Badge Tests ───────────────────────────────────────────

#[test]
fn test_set_verified_sets_status_true() {
    let (env, client, platform_admin, creator, token_address, _token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    // Initially, creator should not be verified
    assert_eq!(client.is_verified(&creator), false);

    // Platform admin sets verified status to true
    client.set_verified(&platform_admin, &creator, &true);

    // Now creator should be verified
    assert_eq!(client.is_verified(&creator), true);
}

#[test]
fn test_set_verified_toggles_status_to_false() {
    let (env, client, platform_admin, creator, token_address, _token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    // Set verified to true first
    client.set_verified(&platform_admin, &creator, &true);
    assert_eq!(client.is_verified(&creator), true);

    // Toggle back to false
    client.set_verified(&platform_admin, &creator, &false);
    assert_eq!(client.is_verified(&creator), false);
}

#[test]
fn test_is_verified_returns_false_for_unverified_creator() {
    let (env, client, platform_admin, creator, token_address, _token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    // Check an unverified creator
    let unverified_creator = Address::generate(&env);
    assert_eq!(client.is_verified(&unverified_creator), false);
}

#[test]
fn test_campaign_info_includes_verified_status() {
    let (env, client, platform_admin, creator, token_address, _token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    // Check campaign info before verification
    let info = client.campaign_info();
    assert_eq!(info.verified, false);
    assert_eq!(info.creator, creator);
    assert_eq!(info.goal, goal);

    // Verify the creator
    client.set_verified(&platform_admin, &creator, &true);

    // Check campaign info after verification
    let info_after = client.campaign_info();
    assert_eq!(info_after.verified, true);
    assert_eq!(info_after.creator, creator);
}

#[test]
#[should_panic(expected = "only platform admin can set verified status")]
fn test_set_verified_rejects_non_admin() {
    let env = Env::default();
    let contract_id = env.register(CrowdfundContract, ());
    let client = CrowdfundContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token_contract_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_address = token_contract_id.address();

    let platform_admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let non_admin = Address::generate(&env);

    env.mock_all_auths();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    env.mock_all_auths_allowing_non_root_auth();
    env.set_auths(&[]);

    client.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &non_admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "set_verified",
            args: soroban_sdk::vec![&env, non_admin.clone(), creator.clone(), true],
            sub_invokes: &[],
        },
    }]);

    // This should panic because non_admin is not the platform admin
    client.set_verified(&non_admin, &creator, &true);
}

// ── Verified Creator Badge Tests ───────────────────────────────────────────

#[test]
fn test_set_verified_sets_status_true() {
    let (env, client, platform_admin, creator, token_address, _token_admin) = setup_env();
    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);
    assert_eq!(client.is_verified(&creator), false);
    client.set_verified(&platform_admin, &creator, &true);
    assert_eq!(client.is_verified(&creator), true);
}

#[test]
fn test_set_verified_toggles_status_to_false() {
    let (env, client, platform_admin, creator, token_address, _token_admin) = setup_env();
    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);
    client.set_verified(&platform_admin, &creator, &true);
    assert_eq!(client.is_verified(&creator), true);
    client.set_verified(&platform_admin, &creator, &false);
    assert_eq!(client.is_verified(&creator), false);
}

#[test]
fn test_is_verified_returns_false_for_unverified_creator() {
    let (env, client, platform_admin, creator, token_address, _token_admin) = setup_env();
    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);
    let unverified_creator = Address::generate(&env);
    assert_eq!(client.is_verified(&unverified_creator), false);
}

#[test]
fn test_campaign_info_includes_verified_status() {
    let (env, client, platform_admin, creator, token_address, _token_admin) = setup_env();
    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);
    let info = client.campaign_info();
    assert_eq!(info.verified, false);
    assert_eq!(info.creator, creator);
    client.set_verified(&platform_admin, &creator, &true);
    let info_after = client.campaign_info();
    assert_eq!(info_after.verified, true);
}

#[test]
#[should_panic(expected = "only platform admin can set verified status")]
fn test_set_verified_rejects_non_admin() {
    let env = Env::default();
    let contract_id = env.register(CrowdfundContract, ());
    let client = CrowdfundContractClient::new(&env, &contract_id);
    let token_admin = Address::generate(&env);
    let token_contract_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_address = token_contract_id.address();
    let platform_admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let non_admin = Address::generate(&env);
    env.mock_all_auths();
    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);
    env.mock_all_auths_allowing_non_root_auth();
    env.set_auths(&[]);
    client.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &non_admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "set_verified",
            args: soroban_sdk::vec![&env, non_admin.clone(), creator.clone(), true],
            sub_invokes: &[],
        },
    }]);
    client.set_verified(&non_admin, &creator, &true);
}

// ── Verified Creator Badge Tests ───────────────────────────────────────────

#[test]
fn test_set_verified_sets_status_true() {
    let (env, client, platform_admin, creator, token_address, _token_admin) = setup_env();
    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);
    assert_eq!(client.is_verified(&creator), false);
    client.set_verified(&platform_admin, &creator, &true);
    assert_eq!(client.is_verified(&creator), true);
}

#[test]
fn test_set_verified_toggles_status_to_false() {
    let (env, client, platform_admin, creator, token_address, _token_admin) = setup_env();
    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);
    client.set_verified(&platform_admin, &creator, &true);
    assert_eq!(client.is_verified(&creator), true);
    client.set_verified(&platform_admin, &creator, &false);
    assert_eq!(client.is_verified(&creator), false);
}

#[test]
fn test_is_verified_returns_false_for_unverified_creator() {
    let (env, client, platform_admin, creator, token_address, _token_admin) = setup_env();
    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);
    let unverified_creator = Address::generate(&env);
    assert_eq!(client.is_verified(&unverified_creator), false);
}

#[test]
fn test_campaign_info_includes_verified_status() {
    let (env, client, platform_admin, creator, token_address, _token_admin) = setup_env();
    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);
    let info = client.campaign_info();
    assert_eq!(info.verified, false);
    assert_eq!(info.creator, creator);
    client.set_verified(&platform_admin, &creator, &true);
    let info_after = client.campaign_info();
    assert_eq!(info_after.verified, true);
}

#[test]
#[should_panic(expected = "only platform admin can set verified status")]
fn test_set_verified_rejects_non_admin() {
    let env = Env::default();
    let contract_id = env.register(CrowdfundContract, ());
    let client = CrowdfundContractClient::new(&env, &contract_id);
    let token_admin = Address::generate(&env);
    let token_contract_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_address = token_contract_id.address();
    let platform_admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let non_admin = Address::generate(&env);
    env.mock_all_auths();
    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);
    env.mock_all_auths_allowing_non_root_auth();
    env.set_auths(&[]);
    client.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &non_admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "set_verified",
            args: soroban_sdk::vec![&env, non_admin.clone(), creator.clone(), true],
            sub_invokes: &[],
        },
    }]);
    client.set_verified(&non_admin, &creator, &true);
}

// ── Verified Creator Badge Tests ───────────────────────────────────────────

#[test]
fn test_set_verified_sets_status_true() {
    let (env, client, platform_admin, creator, token_address, _token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    assert_eq!(client.is_verified(&creator), false);
    client.set_verified(&platform_admin, &creator, &true);
    assert_eq!(client.is_verified(&creator), true);
}

#[test]
fn test_set_verified_toggles_status_to_false() {
    let (env, client, platform_admin, creator, token_address, _token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    client.set_verified(&platform_admin, &creator, &true);
    assert_eq!(client.is_verified(&creator), true);
    client.set_verified(&platform_admin, &creator, &false);
    assert_eq!(client.is_verified(&creator), false);
}

#[test]
fn test_is_verified_returns_false_for_unverified_creator() {
    let (env, client, platform_admin, creator, token_address, _token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    let unverified_creator = Address::generate(&env);
    assert_eq!(client.is_verified(&unverified_creator), false);
}

#[test]
fn test_campaign_info_includes_verified_status() {
    let (env, client, platform_admin, creator, token_address, _token_admin) = setup_env();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    let info = client.campaign_info();
    assert_eq!(info.verified, false);
    assert_eq!(info.creator, creator);
    assert_eq!(info.goal, goal);

    client.set_verified(&platform_admin, &creator, &true);

    let info_after = client.campaign_info();
    assert_eq!(info_after.verified, true);
    assert_eq!(info_after.creator, creator);
}

#[test]
#[should_panic(expected = "only platform admin can set verified status")]
fn test_set_verified_rejects_non_admin() {
    let env = Env::default();
    let contract_id = env.register(CrowdfundContract, ());
    let client = CrowdfundContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token_contract_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_address = token_contract_id.address();

    let platform_admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let non_admin = Address::generate(&env);

    env.mock_all_auths();

    let deadline = env.ledger().timestamp() + 3600;
    let goal: i128 = 1_000_000;
    let min_contribution: i128 = 1_000;
    client.initialize(&platform_admin, &creator, &token_address, &goal, &deadline, &min_contribution);

    env.mock_all_auths_allowing_non_root_auth();
    env.set_auths(&[]);

    client.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &non_admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "set_verified",
            args: soroban_sdk::vec![&env, non_admin.clone(), creator.clone(), true],
            sub_invokes: &[],
        },
    }]);

    client.set_verified(&non_admin, &creator, &true);
}
