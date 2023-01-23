// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ActiveValidator } from '@mysten/sui.js';
import { useMemo } from 'react';

import { DelegationAmount } from './DelegationAmount';
import { calculateAPY } from './calculateAPY';

import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';
import { Stats } from '~/ui/Stats';
import { getStakedPercent } from '~/utils/getStakedPercent';

type StatsCardProps = {
    validatorData: ActiveValidator;
    totalValidatorStake: string;
    epoch: number | string;
};

export function ValidatorStats({
    validatorData,
    epoch,
    totalValidatorStake,
}: StatsCardProps) {
    // TODO: add missing fields
    // const numberOfDelegators = 0;
    //  const networkStakingParticipation = 0;
    //  const votedLastRound =  0;
    //  const tallyingScore =  0;
    //  const lastNarwhalRound = 0;

    const apy = useMemo(
        () => calculateAPY(validatorData, +epoch),
        [validatorData, epoch]
    );
    const delegatedStake =
        validatorData.fields.delegation_staking_pool.fields
            .delegation_token_supply.fields.value;
    const selfStake = validatorData.fields.stake_amount;
    const totalStake = +selfStake + +delegatedStake;
    const commission = +validatorData.fields.commission_rate * 100;
    const lastEpoch = +epoch;

    const delegatedStakePercentage = useMemo(
        () => getStakedPercent(BigInt(delegatedStake), BigInt(totalStake)),
        [delegatedStake, totalStake]
    );

    const totalRewards =
        +validatorData.fields.delegation_staking_pool.fields.sui_balance -
        +delegatedStake;
    const selfStakePercentage = useMemo(
        () => getStakedPercent(BigInt(selfStake), BigInt(totalStake)),
        [selfStake, totalStake]
    );

    return (
        <div className="flex flex-col items-stretch gap-5 md:flex-row">
            <Card spacing="lg">
                <div className="flex basis-full flex-col gap-8 md:basis-1/3">
                    <Heading
                        as="div"
                        variant="heading4/semibold"
                        color="steel-darker"
                    >
                        SUI Staked on Validator
                    </Heading>
                    <div className="flex flex-col gap-8 lg:flex-row">
                        <Stats
                            label="Staking APY"
                            tooltip="Coming soon"
                            unavailable={apy <= 0}
                        >
                            {apy}%
                        </Stats>
                        <Stats
                            label="Commission"
                            tooltip="Coming soon"
                            unavailable={commission <= 0}
                        >
                            <Heading
                                as="h3"
                                variant="heading2/semibold"
                                color="steel-darker"
                            >
                                {commission}%
                            </Heading>
                        </Stats>
                        <Stats label="Total SUI Staked" tooltip="Coming soon">
                            <DelegationAmount amount={totalStake} isStats />
                        </Stats>
                    </div>
                    <div className="flex flex-col gap-8 lg:flex-row">
                        <Stats
                            label="Delegators"
                            tooltip="Coming soon"
                            unavailable
                        />

                        <Stats label="Delegated Staked" tooltip="Coming soon">
                            {delegatedStakePercentage}%
                        </Stats>
                        <Stats label="Self Staked" tooltip="Coming soon">
                            <Heading
                                as="h3"
                                variant="heading3/semibold"
                                color="steel-darker"
                            >
                                {selfStakePercentage}%
                            </Heading>
                        </Stats>
                    </div>
                </div>
            </Card>

            <Card spacing="lg">
                <div className="flex basis-full flex-col items-stretch gap-8 md:basis-80">
                    <Heading
                        as="div"
                        variant="heading4/semibold"
                        color="steel-darker"
                    >
                        Validator Staking Rewards
                    </Heading>
                    <div className="flex flex-col gap-8">
                        <Stats label="Last SUI Epoch" tooltip="Coming soon">
                            <Heading
                                as="div"
                                variant="heading4/semibold"
                                color="steel-darker"
                            >
                                {lastEpoch}
                            </Heading>
                        </Stats>

                        <Stats
                            label="Total SUI Rewards"
                            tooltip="Coming soon"
                            unavailable={+totalRewards <= 0}
                        >
                            <DelegationAmount amount={totalRewards} isStats />
                        </Stats>
                    </div>
                </div>
            </Card>

            <Card spacing="lg">
                <div className="flex max-w-full flex-col gap-8">
                    <Heading
                        as="div"
                        variant="heading4/semibold"
                        color="steel-darker"
                    >
                        Network Participation
                    </Heading>
                    <div className="flex flex-col gap-8">
                        <div className="flex flex-col gap-8 lg:flex-row">
                            <Stats
                                label="Checkpoint Participation"
                                tooltip="Coming soon"
                                unavailable
                            />

                            <Stats
                                label="Voted Last Round"
                                tooltip="Coming soon"
                                unavailable
                            />
                        </div>
                        <div className="flex flex-col gap-8 lg:flex-row">
                            <Stats
                                label="Tallying Score"
                                tooltip="Coming soon"
                                unavailable
                            />
                            <Stats
                                label="Last Narwhal Round"
                                tooltip="Coming soon"
                                unavailable
                            />
                        </div>
                    </div>
                </div>
            </Card>
        </div>
    );
}
