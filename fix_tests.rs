use std::fs;

fn main() {
    let files = [
        "escrow/src/tests/paginated_views.rs",
        "escrow/src/tests/collateral.rs",
        "escrow/src/tests/attestations.rs",
        "escrow/src/tests/allowlist.rs",
        "escrow/src/tests/fees.rs",
        "escrow/src/tests/admin.rs",
        "escrow/src/tests/settlement.rs",
        "escrow/src/tests/funding.rs",
        "escrow/src/tests/yield_tier.rs"
    ];
    for file in files {
        if let Ok(mut text) = fs::read_to_string(file) {
            if text.contains("Address::generate") && !text.contains("testutils::Address") {
                text = text.replace("use soroban_sdk::{", "use soroban_sdk::{testutils::Address, ");
                fs::write(file, text).unwrap();
                println!("Fixed {}", file);
            }
        }
    }
}
