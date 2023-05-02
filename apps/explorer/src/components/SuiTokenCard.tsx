// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatAmount } from '@mysten/core';
import { Sui, Refresh16 } from '@mysten/icons';

import { StatsWrapper } from './HomeMetrics/FormattedStatsAmount';

import { useSuiCoinData } from '~/hooks/useSuiCoinData';
import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';
import { IconButton } from '~/ui/IconButton';
import { Text } from '~/ui/Text';
import { useState } from 'react';

export function SuiTokenCard() {
    const { data, isLoading, refetch } = useSuiCoinData();
    const [isRefreshButtonHovered, setRefreshButtonHovered] = useState(false);
    const formattedPrice = data
        ? data.currentPrice.toLocaleString('en', {
              style: 'currency',
              currency: 'USD',
          })
        : '--';

    return (
        <Card bg="lightBlue" spacing="lg">
            <div className="flex items-center gap-2 md:max-lg:max-w-[336px]">
                <div className="h-4.5 w-4.5 rounded-full bg-sui p-1">
                    <Sui className="h-full w-full text-white" />
                </div>
                <div className="flex w-full flex-col gap-1 sm:flex-row sm:items-center sm:justify-between">
                    <div className="flex items-center gap-2">
                        <Heading
                            as="div"
                            variant="heading4/semibold"
                            color="steel-darker"
                        >
                            1 SUI = {formattedPrice}
                        </Heading>
                        {data?.priceChangePercentageOver24H && (
                            <Heading
                                as="div"
                                variant="heading6/medium"
                                color="issue"
                            >
                                {data.priceChangePercentageOver24H > 0
                                    ? '+'
                                    : null}
                                {data.priceChangePercentageOver24H.toFixed(2)}%
                            </Heading>
                        )}
                    </div>
                    <Text variant="subtitleSmallExtra/medium" color="steel">
                        via CoinGecko
                    </Text>
                </div>
            </div>
            <div className="mt-8 flex w-full gap-8 sm:justify-between md:max-lg:max-w-[336px]">
                <StatsWrapper
                    label="Market Cap"
                    size="sm"
                    postfix="USD"
                    unavailable={isLoading}
                >
                    {formatAmount(data?.fullyDilutedMarketCap)}
                </StatsWrapper>
                <StatsWrapper
                    label="Total Supply"
                    size="sm"
                    postfix="SUI"
                    unavailable={isLoading}
                >
                    {formatAmount(data?.totalSupply)}
                </StatsWrapper>
                <IconButton
                    icon={Refresh16}
                    aria-label="Refresh SUI coin data"
                    onClick={() => refetch()}
                    onMouseEnter={() => setRefreshButtonHovered(true)}
                    onMouseLeave={() => setRefreshButtonHovered(false)}
                />
            </div>
        </Card>
    );
}
