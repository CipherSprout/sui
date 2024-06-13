#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

echo "sui-test-validator binary has been deprecated in favor of sui start, which is a more powerful command that allows you to start the local network with more options.
This script offers backward compatibiltiy, but ideally, you should migrate to sui start instead. Use sui start --help to see all the flags and options. 

To recreate the exact basic functionality of sui-test-validator, you must use the following options:
  * --with-faucet --> to start the faucet server on the default host and port
  * --random-genesis --> to start the local network without persisting the state and from a random genesis

You can also use the following options to start the local network with more features:
  * --with-indexer --> to start the indexer on the default host and port. Note that this requires a Postgres database to be running locally, or you need to set the different options to connect to a remote indexer database.
  * --with-graphql --> to start the GraphQL server on the default host and port"

# In sui-test-validator the graphql is started by passing the graphql-port argument
graphql_port="--graphql-port"
config_dir=false
start_graphql=false

# holds the args names
named_args=()



export RUST_LOG=info

# Loop through all arguments
for arg in "$@"; do
    if [ "$arg" == "--config-dir" ]; then
        config_dir=true
        named_args+=("--network.config")
        continue
    fi
    if [ "$arg" == "$graphql_port" ]; then
        start_graphql=true
    fi
    named_args+=("$arg")

done

cmd="sui start --with-faucet --random-genesis"
# To maintain compatibility, when passing a network configuration in a directory, --random-genesis cannot be passed.
if [ "$config_dir" = true ]; then
    echo "Starting with the provided network configuration."
    cmd="sui start --with-faucet"
fi

if  [ "$start_graphql" = true ]; then
    echo "Starting with GraphQL enabled."
    cmd+=" --with-graphql"
fi
echo "Running command: $cmd ${named_args[@]}"
$cmd "${named_args[@]}"

