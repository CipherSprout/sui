// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    is,
    SuiObject,
    type ValidatorsFields,
    type ActiveValidator,
} from '@mysten/sui.js';
import cl from 'classnames';
import { useState, useMemo } from 'react';

import { calculateAPY } from '../calculateAPY';
import { STATE_OBJECT, getName } from '../usePendingDelegation';
import { ValidatorListItem } from './ValidatorListItem';
import { Content, Menu } from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import { Text } from '_app/shared/text';
import Alert from '_components/alert';
import Icon, { SuiIcons } from '_components/icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { useGetObject } from '_hooks';

function validatorsList(validator: ActiveValidator[], epoch: number) {
    return validator.map((validator) => ({
        name: getName(validator.fields.metadata.fields.name),
        address: validator.fields.metadata.fields.sui_address,
        apy: calculateAPY(validator, epoch),
    }));
}

const collator = new Intl.Collator('en', {
    sensitivity: 'base',
    numeric: true,
});

export function SelectValidatorCard() {
    const [selectedValidator, setSelectedValidator] = useState<null | string>(
        null
    );
    const [sortKey, setSortKey] = useState<'name' | 'apy'>('apy');
    const [sortAscending, setSortAscending] = useState(true);

    const { data, isLoading, isError } = useGetObject(STATE_OBJECT);

    const validatorsData =
        data &&
        is(data.details, SuiObject) &&
        data.details.data.dataType === 'moveObject'
            ? (data.details.data.fields as ValidatorsFields)
            : null;

    const selectValidator = (address: string) => {
        setSelectedValidator((state) => (state !== address ? address : null));
    };

    const handleSortByKey = (key: 'name' | 'apy') => {
        setSortKey(key);
        setSortAscending(!sortAscending);
    };

    const validatorList = useMemo(() => {
        if (!validatorsData) return [];

        return validatorsList(
            validatorsData.validators.fields.active_validators,
            +validatorsData.epoch
        ).sort((a, b) => {
            if (sortKey === 'name') {
                return sortAscending
                    ? collator.compare(a[sortKey], b[sortKey])
                    : collator.compare(b[sortKey], a[sortKey]);
            }
            return sortAscending
                ? b[sortKey] - a[sortKey]
                : a[sortKey] - b[sortKey];
        });
    }, [sortAscending, sortKey, validatorsData]);

    if (isLoading) {
        return (
            <div className="p-2 w-full flex justify-center items-center h-full">
                <LoadingIndicator />
            </div>
        );
    }

    if (isError) {
        return (
            <div className="p-2">
                <Alert mode="warning">
                    <div className="mb-1 font-semibold">
                        Something went wrong
                    </div>
                </Alert>
            </div>
        );
    }

    return (
        <div className="flex flex-col w-full -my-5">
            <Content className="flex flex-col w-full items-center">
                <div className="flex flex-col w-full items-center -top-5 bg-white sticky pt-5 pb-2.5 z-50 mt-0">
                    <div className="flex items-start w-full mb-2">
                        <Text variant="subtitle" weight="medium" color="hero">
                            Sort by:
                        </Text>
                        <div className="flex items-center ml-2 gap-1.5">
                            <button
                                className="bg-transparent border-0 p-0 flex gap-1 cursor-pointer w-10"
                                onClick={() => handleSortByKey('apy')}
                            >
                                <Text
                                    variant="caption"
                                    weight="medium"
                                    color="steel-darker"
                                >
                                    APY
                                </Text>
                                {sortKey === 'apy' && (
                                    <Icon
                                        icon={SuiIcons.ArrowLeft}
                                        className={cl(
                                            'text-captionSmall font-thin  text-steel-darker',
                                            sortAscending
                                                ? '-rotate-90'
                                                : 'rotate-90'
                                        )}
                                    />
                                )}
                            </button>

                            <button
                                className="bg-transparent border-0 p-0 flex gap-1 cursor-pointer"
                                onClick={() => handleSortByKey('name')}
                            >
                                <Text
                                    variant="caption"
                                    weight="medium"
                                    color="steel-darker"
                                >
                                    Name
                                </Text>
                                {sortKey === 'name' && (
                                    <Icon
                                        icon={SuiIcons.ArrowLeft}
                                        className={cl(
                                            'text-captionSmall font-thin  text-steel-darker',
                                            sortAscending
                                                ? '-rotate-90'
                                                : 'rotate-90'
                                        )}
                                    />
                                )}
                            </button>
                        </div>
                    </div>

                    <div className="flex items-start w-full">
                        <Text
                            variant="subtitle"
                            weight="medium"
                            color="steel-darker"
                        >
                            Select a validator to start staking SUI.
                        </Text>
                    </div>
                </div>
                <div className="flex items-start flex-col w-full mt-1 flex-1">
                    {validatorsData &&
                        validatorList.map(({ name, address, apy }) => (
                            <div
                                className="cursor-pointer w-full relative"
                                key={address}
                                onClick={() => selectValidator(address)}
                            >
                                <ValidatorListItem
                                    selected={selectedValidator === address}
                                    validatorAddress={address}
                                    validatorName={name}
                                    apy={apy}
                                />
                            </div>
                        ))}
                </div>
            </Content>
            {selectedValidator && (
                <Menu
                    stuckClass="staked-cta"
                    className="w-full px-0 pb-5 mx-0 -bottom-5"
                >
                    <Button
                        size="large"
                        mode="primary"
                        href={`/stake/new?address=${encodeURIComponent(
                            selectedValidator
                        )}`}
                        className="w-full"
                    >
                        Select Amount
                        <Icon
                            icon={SuiIcons.ArrowRight}
                            className="text-captionSmall text-white font-normal"
                        />
                    </Button>
                </Menu>
            )}
        </div>
    );
}
