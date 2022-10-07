// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useMemo, useCallback } from 'react';
import { useIntl } from 'react-intl';
import { useNavigate, Link } from 'react-router-dom';

import Icon, { SuiIcons } from '_components/icon';
import { useAppSelector } from '_hooks';
import { accountAggregateBalancesSelector } from '_redux/slices/account';
import { GAS_TYPE_ARG, Coin } from '_redux/slices/sui-objects/Coin';
import { balanceFormatOptions } from '_shared/formatting';

import st from './ActiveCoinsCard.module.scss';

// Get all the coins that are available in the account.
// default coin type is GAS_TYPE_ARG unless specified in props
// create a list of coins that are available in the account
function ActiveCoinsCard({
    activeCoinType = GAS_TYPE_ARG,
    showActiveCoin = true,
}: {
    activeCoinType: string;
    showActiveCoin?: boolean;
}) {
    const intl = useIntl();
    const aggregateBalances = useAppSelector(accountAggregateBalancesSelector);

    const allCoins = useMemo(
        () =>
            Object.entries(aggregateBalances).map((aType) => {
                const name = Coin.getCoinSymbol(aType[0]);
                return {
                    coinName: name,
                    coinSymbol: name,
                    coinType: aType[0],
                    //TODO: default coin icon switch to on chain metadata
                    coinIconName:
                        GAS_TYPE_ARG === aType[0]
                            ? SuiIcons.SuiLogoIcon
                            : SuiIcons.Tokens,
                    type: aType,
                    balance: aType[1],
                };
            }),
        [aggregateBalances]
    );

    const coins = useMemo(() => {
        return allCoins.map((coin) => {
            const balance = intl.formatNumber(
                BigInt(coin.balance || 0),
                balanceFormatOptions
            );
            return {
                ...coin,
                balance,
            };
        });
    }, [allCoins, intl]);

    const activeCoin = useMemo(() => {
        return coins.filter((coin) => coin.coinType === activeCoinType)[0];
    }, [activeCoinType, coins]);

    const IconName = activeCoin?.coinIconName || SuiIcons.SuiLogoIcon;

    const defaultIconClass =
        GAS_TYPE_ARG !== activeCoin?.coinSymbol ? st.defaultCoin : '';

    const SelectedCoinCard = activeCoin ? (
        <div className={st.selectCoin}>
            <Link
                to={`/send/select?${new URLSearchParams({
                    type: activeCoinType,
                }).toString()}`}
                className={st.coin}
            >
                <div className={cl(st.suiIcon, defaultIconClass)}>
                    <Icon icon={IconName} />
                </div>
                <div className={st.coinLabel}>
                    {activeCoin?.coinName}{' '}
                    <span className={st.coinSymbol}>
                        {activeCoin.coinSymbol}
                    </span>
                </div>
                <div className={st.chevron}>
                    <Icon icon={SuiIcons.SuiChevronRight} />
                </div>
            </Link>
            <div className={st.coinBalance}>
                <div className={st.coinBalanceLabel}>Total Available</div>
                <div className={st.coinBalanceValue}>
                    {activeCoin.balance} {activeCoin.coinSymbol}
                </div>
            </div>
        </div>
    ) : null;

    const navigate = useNavigate();

    const changeCoinType = useCallback(
        (event: React.MouseEvent<HTMLDivElement>) => {
            const cointype = event.currentTarget.dataset.cointype as string;
            navigate(
                `/send?${new URLSearchParams({
                    type: cointype,
                }).toString()}`
            );
        },
        [navigate]
    );

    const CoinListCard = (
        <div className={st.coinList}>
            {coins.map((coin, index) => (
                <div
                    className={st.coinDetail}
                    key={index}
                    onClick={changeCoinType}
                    data-cointype={coin.coinType}
                >
                    <div className={cl(st.coinIcon, defaultIconClass)}>
                        <Icon icon={coin.coinIconName} />
                    </div>
                    <div className={st.coinLabel}>
                        {coin.coinName} <span>{coin.coinSymbol}</span>
                    </div>
                    <div className={st.coinAmount}>
                        {coin.balance} <span>{coin.coinSymbol}</span>
                    </div>
                </div>
            ))}
        </div>
    );

    return (
        <div className={st.content}>
            {showActiveCoin ? SelectedCoinCard : CoinListCard}
        </div>
    );
}

export default ActiveCoinsCard;
