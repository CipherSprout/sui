#!/bin/bash

echo "sui-test-validator binary has been deprecated in favor of sui start, which is a more powerful command that allows you to start the local network with more options.
This script offers backward compatibiltiy, but ideally, you should migrate to sui start instead. Use sui start --help to see all the flags and options. 

To recreate the exact basic functionality of sui-test-validator, you must use the following options:
  * --with-faucet --> to start the faucet server on the default host and port
  * --random-genesis --> to start the local network without persisting the state and from a random genesis

You can also use the following options to start the local network with more features:
  * --with-indexer --> to start the indexer on the default host and port. Note that this requires a Postgres database to be running
  * --with-graphql --> to start the GraphQL server on the default host and port"

# In sui-test-validator the graphql is started by passing the graphql-port argument
graphql_port="--graphql-port"
start_graphql=false

export RUST_LOG=info

# Loop through all arguments
for arg in "$@"; do
    if [ "$arg" == "$graphql_port" ]; then
        start_graphql=true
        break
    fi
done

cmd="sui start --with-faucet --random-genesis"

if  [ "$start_graphql" = true ]; then
    echo "Starting with GraphQL enabled."
    cmd+=" --with-graphql"
fi

$cmd "$@"

