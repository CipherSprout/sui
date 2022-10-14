// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { JsonRpcProvider, LATEST_RPC_API_VERSION } from '@mysten/sui.js';

import { getEndpoint, Network } from './rpcSetting';

export { Network, getEndpoint };

const defaultRpcMap: Map<Network | string, JsonRpcProvider> = new Map();
export const DefaultRpcClient = (network: Network | string) => {
    const existingClient = defaultRpcMap.get(network);
    if (existingClient) return existingClient;

    const provider = new JsonRpcProvider(
        getEndpoint(network),
        true,
        LATEST_RPC_API_VERSION
    );
    defaultRpcMap.set(network, provider);
    return provider;
};
