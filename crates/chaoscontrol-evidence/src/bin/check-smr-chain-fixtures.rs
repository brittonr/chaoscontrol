fn main() {
    if let Err(error) = chaoscontrol_smr::smr_chain_selftest() {
        eprintln!("smr-chain-fixtures: FAIL: {error}");
        std::process::exit(1);
    }
    println!("smr-chain-fixtures: PASS");
}
