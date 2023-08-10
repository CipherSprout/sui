// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 *  ######################################
 *  ### DO NOT EDIT THIS FILE DIRECTLY ###
 *  ######################################
 *
 * This file is generated from:
 * /crates/sui-open-rpc/spec/openrpc.json
 */

import type { QueryEventsParams } from '@mysten/sui.js/client';
import type { UseSuiClientQueryOptions } from '../useSuiClientQuery.js';
import { useSuiClientQuery } from '../useSuiClientQuery.js';

export function useEventsQuery(
	params: QueryEventsParams,
	options?: UseSuiClientQueryOptions<'queryEvents'>,
) {
	return useSuiClientQuery(
		{
			method: 'queryEvents',
			params,
		},
		options,
	);
}
