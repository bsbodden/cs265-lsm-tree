#!/bin/bash

# Exit on error
set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m' # No Color

# Step 1: Generate workload
echo -e "${GREEN}Generating workload...${NC}"
cd generator
gcc -o generator generator.c
./generator -o workload.txt -n 1000
cd ..

# Step 2: Start the server
echo -e "${GREEN}Starting server...${NC}"
cargo build --release
./target/release/server &
SERVER_PID=$!

# Step 3: Run integration tests
echo -e "${GREEN}Running integration tests...${NC}"
sleep 2 # Allow server to initialize
cargo test -- --nocapture || {
    echo -e "${RED}Tests failed. Stopping server...${NC}"
    kill $SERVER_PID
    exit 1
}

# Step 4: Evaluate results
echo -e "${GREEN}Evaluating server output...${NC}"
python3 generator/evaluate.py --workload generator/workload.txt --output server_output.txt

# Step 5: Stop the server
echo -e "${GREEN}Stopping server...${NC}"
kill $SERVER_PID

echo -e "${GREEN}All steps completed successfully!${NC}"
