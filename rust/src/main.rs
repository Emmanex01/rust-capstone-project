#![allow(unused)]
use bitcoin::hex::DisplayHex;
use bitcoincore_rpc::bitcoin::Amount;
use bitcoincore_rpc::{Auth, Client, RpcApi};
use serde::Deserialize;
use serde_json::json;
use std::fs::File;
use std::io::Write;
use std::collections::HashSet;

// Node access params
const RPC_URL: &str = "http://127.0.0.1:18443"; // Default regtest RPC port
const RPC_USER: &str = "alice";
const RPC_PASS: &str = "password";

// You can use calls not provided in RPC lib API using the generic `call` function.
// An example of using the `send` RPC call, which doesn't have exposed API.
// You can also use serde_json `Deserialize` derivation to capture the returned json result.
fn send(rpc: &Client, addr: &str) -> bitcoincore_rpc::Result<String> {
    let args = [
        json!([{addr : 100 }]), // recipient address
        json!(null),            // conf target
        json!(null),            // estimate mode
        json!(null),            // fee rate in sats/vb
        json!(null),            // Empty option object
    ];

    #[derive(Deserialize)]
    struct SendResult {
        complete: bool,
        txid: String,
    }
    let send_result = rpc.call::<SendResult>("send", &args)?;
    assert!(send_result.complete);
    Ok(send_result.txid)
}

// Helper function
fn create_or_load_wallet(rpc: &Client, wallet_name: &str) -> bitcoincore_rpc::Result<()> {
        // check if the wallet is loaded before
        if rpc.list_wallets()?.contains(&wallet_name.to_string()) {
            println!("wallet {} is already loaded", wallet_name);
            return Ok(());
        }

        // if it is not loaded before
        match rpc.load_wallet(wallet_name) {
            Ok(_) => {
                    println!("Successfully loaded existing wallet '{}' from disk.", wallet_name);
                    Ok(())
                }
            // If loading fails because it doesn't exist, create it.
            Err(e) => {
                println!("Wallet '{}' not found on disk. Creating a new one.", wallet_name);
                rpc.create_wallet(wallet_name, None, None, None, None)?;
                println!("Wallet '{}' created successfully.", wallet_name);
                Ok(())
            }

             // Handle other potential errors during loading.
            Err(e) => Err(e)
        }
    }

fn main() -> bitcoincore_rpc::Result<()> {
    // Connect to Bitcoin Core RPC
    let rpc = Client::new(
        RPC_URL,
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    // Get blockchain info
    let blockchain_info = rpc.get_blockchain_info()?;
    println!("Blockchain Info: {:?}", blockchain_info);

    // Create/Load the wallets, named 'Miner' and 'Trader'. Have logic to optionally create/load them if they do not exist or not loaded already.
    let miner_wallet_name = "Miner";
    let trader_wallet_name = "Trader";
    create_or_load_wallet(&rpc, miner_wallet_name)?;
    create_or_load_wallet(&rpc, trader_wallet_name)?;

    // We create wallet-specific RPC clients url for easier management.
    println!("Creating wallet-specific RPC clients...");
    let miner_auth = Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned());
    let miner_rpc = Client::new(&format!("{}/wallet/{}", RPC_URL, "Miner"), miner_auth)?;
    
    let trader_auth = Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned());
    let trader_rpc = Client::new(&format!("{}/wallet/{}", RPC_URL, "Trader"), trader_auth)?;
    
    println!("'Miner' and 'Trader' wallets are ready.");


    // Generate spendable balances in the Miner wallet. How many blocks needs to be mined?
    let miner_address = miner_rpc.get_new_address(None, None)?.assume_checked();
    let initial_balance = miner_rpc.get_balance(None, None)?;
    if initial_balance < Amount::from_btc(50.0)? {
        println!("Miner balance is low. Mining 101 blocks to mature coinbase rewards...");
        let block_hashes = miner_rpc.generate_to_address(101, &miner_address)?;
        println!("Mined {} blocks. First new block hash: {}", block_hashes.len(), block_hashes[0]);
    } else {
        println!("Miner already has a sufficient balance.");
    }
    let balance = miner_rpc.get_balance(None, None)?;
    println!("Miner wallet balance: {} BTC", balance.to_btc());

    // Load Trader wallet and generate a new address
    let trader_address = trader_rpc.get_new_address(None, None)?.assume_checked();
    println!("Generated new address for Trader: {}", trader_address);

    // Send 20 BTC from Miner to Trader
    let amount_to_send = Amount::from_btc(20.0)?;
    println!("Sending {} BTC from Miner to Trader...", amount_to_send.to_btc());
    let txid = miner_rpc.send_to_address(&trader_address, amount_to_send, None, None, None, None, None, None)?;
    println!("Transaction sent! TXID: {}", txid);

    // Check transaction in mempool
    let mempool = rpc.get_raw_mempool()?;
    if mempool.contains(&txid) {
        println!("Success! Transaction {} found in mempool.", txid);
    } else {
        println!("Error! Transaction not found in mempool.");
        // This would be an unexpected error in this script.
    }

    // Mine 1 block to confirm the transaction
    let block_hash = miner_rpc.generate_to_address(1, &miner_address)?[0];
    println!("Block {} mined, confirming the transaction.", block_hash);

    // Extract all required transaction details
    let tx_info = rpc.get_raw_transaction_info(&txid, Some(&block_hash))?;
    println!("Successfully fetched confirmed transaction details.");

    // 1. Get block details
    let block_header_info = rpc.get_block_header_info(&tx_info.blockhash.unwrap())?;
    let block_height = block_header_info.height as u64;

    // 2. Calculate total input value and find input addresses
    let mut total_input_value = Amount::ZERO;
    let mut miner_input_addresses = HashSet::new(); // Use a HashSet to store unique addresses

    for vin in &tx_info.vin {
        if let (Some(prev_txid), Some(prev_vout)) = (vin.txid, vin.vout) {
            // Fetch the previous transaction that this input is spending from
            let prev_tx_info = rpc.get_raw_transaction_info(&prev_txid, None)?;
            let spent_output = &prev_tx_info.vout[prev_vout as usize];
            
            total_input_value += spent_output.value;
            if let Some(address) = &spent_output.script_pub_key.address {
                miner_input_addresses.insert(address.clone());
            }
        }
    }
    let input_addresses_str = miner_input_addresses.iter().map(|a| a.clone().assume_checked().to_string()).collect::<Vec<_>>().join(", ");


    // 3. Calculate total output value and identify Trader/Change outputs
    let mut total_output_value = Amount::ZERO;
    let mut trader_output = None;
    let mut miner_change_output = None;

     for vout in &tx_info.vout {
        total_output_value += vout.value;
        // Use `if let` to safely unwrap the address from the output
        if let Some(output_address) = &vout.script_pub_key.address {
            // Now, compare the inner values after converting the unchecked one
            if output_address.clone().assume_checked() == trader_address {
                trader_output = Some((vout.value, output_address.clone()));
            } else {
                miner_change_output = Some((vout.value, output_address.clone()));
            }
        }
    }

    // 4. Calculate fees
    let transaction_fee = total_input_value - total_output_value;

    // Write the data to ../out.txt in the specified format given in readme.md
    let mut output_string = String::new();
    
    output_string.push_str(&format!("Transaction ID (txid): {}\n", tx_info.txid));
    output_string.push_str(&format!("Miner's Input Address: {}\n", input_addresses_str));
    output_string.push_str(&format!("Miner's Input Amount (in BTC): {}\n", total_input_value.to_btc()));

    if let Some((amount, address)) = trader_output {
        output_string.push_str(&format!("Trader's Output Address: {}\n", address.assume_checked()));
        output_string.push_str(&format!("Trader's Output Amount (in BTC): {}\n", amount.to_btc()));
    }

    if let Some((amount, address)) = miner_change_output {
        output_string.push_str(&format!("Miner's Change Address: {}\n", address.assume_checked()));
        output_string.push_str(&format!("Miner's Change Amount (in BTC): {}\n", amount.to_btc()));
    } else {
        output_string.push_str("Miner's Change Address: None\n");
        output_string.push_str("Miner's Change Amount (in BTC): 0.0\n");
    }

    output_string.push_str(&format!("Transaction Fees (in BTC): {}\n", transaction_fee.to_btc()));
    output_string.push_str(&format!("Block height at which the transaction is confirmed: {}\n", block_height));
    output_string.push_str(&format!("Block hash at which the transaction is confirmed: {}\n", tx_info.blockhash.unwrap()));
    
    let file_path = "../out.txt";
    let mut file = File::create(file_path)?;
    file.write_all(output_string.as_bytes())?;
    println!("Successfully wrote transaction details to {}", file_path);

    println!("\n--- Content of out.txt ---\n{}", output_string);

    Ok(())
}
