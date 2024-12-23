# Compiler and flags
CC = gcc
CFLAGS = -Wall -Wextra -pedantic -I/opt/homebrew/Cellar/gsl/2.8/include
LDFLAGS = -L/opt/homebrew/Cellar/gsl/2.8/lib -lgsl -lgslcblas -lm

# Generator paths
GENERATOR_DIR = generator
GENERATOR_SRC = $(GENERATOR_DIR)/generator.c
GENERATOR_BIN = $(GENERATOR_DIR)/generator
WORKLOAD_FILE = $(GENERATOR_DIR)/workload.txt

# Python evaluator
EVALUATOR = $(GENERATOR_DIR)/evaluate.py

# Targets
all: $(GENERATOR_BIN)

# Compile the generator with GSL dependencies
$(GENERATOR_BIN): $(GENERATOR_SRC)
	$(CC) $(CFLAGS) $< -o $@ $(LDFLAGS)

# Clean up generated files
clean:
	rm -f $(GENERATOR_BIN) $(WORKLOAD_FILE)

# Generate a workload
generate_workload: $(GENERATOR_BIN)
	$(GENERATOR_BIN) \
		--puts 500 \
		--gets 300 \
		--ranges 200 \
		--max-range-size 50 \
		--seed 42 > $(WORKLOAD_FILE)

# Check Python dependencies
check_python_dependencies:
	@python3 -c "import sortedcontainers" || pip3 install sortedcontainers

.PHONY: kill-servers clean-all test

kill-servers:
	@echo "Killing any running server processes..."
	-pkill -f "target/release/server" || true
	@sleep 1

clean-all: clean kill-servers
	cargo clean

test_all: clean-all check_python_dependencies generate_workload
	@echo "Running integration tests..."
	cargo test -- --nocapture
	@echo "Evaluating results..."
	python3 $(EVALUATOR) generator/workload.txt > server_output.txt









