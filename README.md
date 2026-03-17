# Solana Crowdfunding Smart Contract

A decentralized crowdfunding platform built on Solana.

## Overview

This project implements a smart contract for creating and managing crowdfunding campaigns on the Solana blockchain. It provides a secure and transparent way for creators to raise funds and for contributors to support projects they believe in.

## Features

*   **Campaign Creation:** Users can initialize new crowdfunding campaigns, specifying a target funding amount and a deadline.
*   **Contributions:** Anyone can contribute SOL to an active campaign.
*   **Withdrawal:** If a campaign successfully reaches its target funding amount by the deadline, the campaign creator can withdraw the collected funds.
*   **Refunds:** If a campaign fails to reach its target amount by the deadline, contributors can claim a full refund of their contributions.

## Project Structure

*   `src/lib.rs`: Entry point for the Solana program, mapping instructions to their processors.
*   `src/processor.rs`: Contains the core logic for each instruction (Initialize, Contribute, Withdraw, Refund).
*   `src/state.rs`: Defines the data structures stored in the program's accounts (e.g., Campaign details).
*   `src/error.rs`: Custom error types for the program.
*   `tests/integration.rs`: Integration tests to verify the program's functionality.

## Development

### Prerequisites

*   Rust
*   Solana CLI

### Build

To compile the smart contract:

```bash
cargo build-sbf
```

### Test

To run the integration tests:

```bash
cargo test-sbf
```
