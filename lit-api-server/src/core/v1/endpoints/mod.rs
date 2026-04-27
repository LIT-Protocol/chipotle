mod account_management;
mod actions;
mod billing;
mod configuration;

use self::account_management::*;
use self::actions::*;
use self::billing::*;
use self::configuration::*;

use rocket::Route;
use rocket_okapi::okapi::openapi3::OpenApi;
use rocket_okapi::openapi_get_routes_spec;

/// Returns Core v1 routes and the OpenAPI spec for them.
pub fn routes_with_spec() -> (Vec<Route>, OpenApi) {
    openapi_get_routes_spec![
        list_api_keys,
        new_account,
        account_exists,
        create_wallet,
        create_wallet_with_signature,
        lit_action,
        get_lit_action_ipfs_id,
        add_group,
        remove_group,
        add_action,
        delete_action,
        add_action_to_group,
        add_pkp_to_group,
        remove_pkp_from_group,
        add_usage_api_key,
        update_usage_api_key,
        remove_usage_api_key,
        update_group,
        remove_action_from_group,
        update_action_metadata,
        update_usage_api_key_metadata,
        list_groups,
        list_wallets,
        list_wallets_in_group,
        list_actions,
        get_node_chain_config,
        get_chain_config_keys,
        get_lit_action_client_config,
        get_api_payers,
        get_admin_api_payer,
        billing_stripe_config,
        billing_balance,
        billing_create_payment_intent,
        billing_confirm_payment,
        get_version,
    ]
}
