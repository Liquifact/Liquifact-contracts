#!/bin/bash

# Gold Standard Integration Test Runner
echo "🚀 Running LiquiFact Escrow Gold Standard Integration Test"
echo "=================================================="

cd escrow

# Clean any existing build artifacts
echo "🧹 Cleaning build artifacts..."
cargo clean

# Check compilation
echo "🔍 Checking compilation..."
if cargo check; then
    echo "✅ Compilation successful"
else
    echo "❌ Compilation failed"
    exit 1
fi

# Run the gold standard integration test
echo "🧪 Running gold standard integration test..."
if cargo test test_escrow_gold_standard_happy_path_open_overfund_snapshot_settle_claim --lib -- --nocapture; then
    echo "✅ Gold standard test passed"
else
    echo "❌ Gold standard test failed"
    exit 1
fi

# Run the tiered yield test
echo "🧪 Running tiered yield integration test..."
if cargo test test_escrow_tiered_yield_with_commitment_locks --lib -- --nocapture; then
    echo "✅ Tiered yield test passed"
else
    echo "❌ Tiered yield test failed"
    exit 1
fi

# Run all integration tests
echo "🧪 Running all integration tests..."
if cargo test integration --lib -- --nocapture; then
    echo "✅ All integration tests passed"
else
    echo "❌ Some integration tests failed"
    exit 1
fi

echo ""
echo "🎉 All tests completed successfully!"
echo "📋 Summary:"
echo "   ✅ Compilation check passed"
echo "   ✅ Gold standard happy path test passed"
echo "   ✅ Tiered yield commitment test passed"
echo "   ✅ All integration tests passed"
echo ""
echo "🔗 Next steps:"
echo "   1. Run 'cargo test' to run all tests"
echo "   2. Run 'cargo llvm-cov' for coverage analysis"
echo "   3. Submit PR with test output summary"