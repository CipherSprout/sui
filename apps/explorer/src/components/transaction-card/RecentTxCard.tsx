// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    type ExecutionStatusType,
    type TransactionKindName,
} from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';
import cl from 'clsx';
import { useState, useContext, useCallback, useMemo } from 'react';
import { useSearchParams, Link } from 'react-router-dom';

import { ReactComponent as ArrowRight } from '../../assets/SVGIcons/12px/ArrowRight.svg';
import TabFooter from '../../components/tabs/TabFooter';
import { NetworkContext } from '../../context';
import {
    DefaultRpcClient as rpc,
    type Network,
} from '../../utils/api/DefaultRpcClient';
import { IS_STATIC_ENV } from '../../utils/envUtil';
import { getAllMockTransaction } from '../../utils/static/searchUtil';
import Pagination from '../pagination/Pagination';
import {
    type TxnData,
    genTableDataFromTxData,
    getDataOnTxDigests,
} from './TxCardUtils';

import styles from './RecentTxCard.module.css';

import { Banner } from '~/ui/Banner';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { TableCard } from '~/ui/TableCard';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';

const TRUNCATE_LENGTH = 10;
const NUMBER_OF_TX_PER_PAGE = 20;
const DEFAULT_PAGINATION_TYPE = 'more button';

type PaginationType = 'more button' | 'pagination' | 'none';

function generateStartEndRange(
    txCount: number,
    txNum: number,
    pageNum?: number
): { startGatewayTxSeqNumber: number; endGatewayTxSeqNumber: number } {
    // Pagination pageNum from query params - default to 0; No negative values
    const txPaged = pageNum && pageNum > 0 ? pageNum - 1 : 0;
    const endGatewayTxSeqNumber = txCount - txNum * txPaged;
    const startGatewayTxSeqNumber = Math.max(endGatewayTxSeqNumber - txNum, 0);
    return {
        startGatewayTxSeqNumber,
        endGatewayTxSeqNumber,
    };
}

// Static data for development and testing
const getRecentTransactionsStatic = (): Promise<TxnData[]> => {
    return new Promise((resolve) => {
        setTimeout(() => {
            const latestTx = getAllMockTransaction().map((tx) => ({
                ...tx,
                status: tx.status as ExecutionStatusType,
                kind: tx.kind as TransactionKindName,
            }));
            resolve(latestTx as TxnData[]);
        }, 500);
    });
};

// TOD0: Optimize this method to use fewer API calls. Move the total tx count to this component.
async function getRecentTransactions(
    network: Network | string,
    totalTx: number,
    txNum: number,
    pageNum?: number
): Promise<TxnData[]> {
    // If static env, use static data
    if (IS_STATIC_ENV) {
        return getRecentTransactionsStatic();
    }
    // Get the latest transactions
    // Instead of getRecentTransactions, use getTransactionCount
    // then use getTransactionDigestsInRange using the totalTx as the start totalTx sequence number - txNum as the end sequence number
    // Get the total number of transactions, then use as the start and end values for the getTransactionDigestsInRange
    const { endGatewayTxSeqNumber, startGatewayTxSeqNumber } =
        generateStartEndRange(totalTx, txNum, pageNum);

    // TODO: Add error page
    // If paged tx value is less than 0, out of range
    if (endGatewayTxSeqNumber < 0) {
        throw new Error('Invalid transaction number');
    }
    const transactionDigests = await rpc(network).getTransactionDigestsInRange(
        startGatewayTxSeqNumber,
        endGatewayTxSeqNumber
    );

    // result returned by getTransactionDigestsInRange is in ascending order
    const transactionData = await getDataOnTxDigests(
        network,
        [...transactionDigests].reverse()
    );

    // TODO: Don't force the type here:
    return transactionData as TxnData[];
}

type Props = {
    paginationtype?: PaginationType;
    txPerPage?: number;
    truncateLength?: number;
};

export function LatestTxCard({
    truncateLength = TRUNCATE_LENGTH,
    paginationtype = DEFAULT_PAGINATION_TYPE,
    txPerPage: initialTxPerPage,
}: Props) {
    const [txPerPage, setTxPerPage] = useState(
        initialTxPerPage || NUMBER_OF_TX_PER_PAGE
    );

    const [network] = useContext(NetworkContext);
    const [searchParams, setSearchParams] = useSearchParams();

    const [pageIndex, setPageIndex] = useState(
        parseInt(searchParams.get('p') || '1', 10) || 1
    );

    const handlePageChange = useCallback(
        (newPage: number) => {
            setPageIndex(newPage);
            setSearchParams({ p: newPage.toString() });
        },
        [setSearchParams]
    );

    const countQuery = useQuery(['transactions', 'count'], () => {
        return rpc(network).getTotalTransactionNumber();
    });

    const transactionQuery = useQuery(
        ['transactions', { total: countQuery.data, txPerPage, pageIndex }],
        async () => {
            const { data: count } = countQuery;

            if (!count) {
                throw new Error('No transactions found');
            }

            // If pageIndex is greater than maxTxPage, set to maxTxPage
            const maxTxPage = Math.ceil(count / txPerPage);
            const pg = pageIndex > maxTxPage ? maxTxPage : pageIndex;

            return getRecentTransactions(network, count, txPerPage, pg);
        },
        {
            enabled: countQuery.isFetched,
            keepPreviousData: true,
        }
    );

    const recentTx = useMemo(
        () =>
            transactionQuery.data
                ? genTableDataFromTxData(transactionQuery.data, truncateLength)
                : null,
        [transactionQuery.data, truncateLength]
    );

    const stats = {
        count: countQuery?.data || 0,
        stats_text: 'Total transactions',
    };

    const PaginationWithStatsOrStatsWithLink =
        paginationtype === 'pagination' ? (
            <Pagination
                totalItems={countQuery?.data || 0}
                itemsPerPage={txPerPage}
                updateItemsPerPage={setTxPerPage}
                onPagiChangeFn={handlePageChange}
                currentPage={pageIndex}
                stats={stats}
            />
        ) : (
            <TabFooter stats={stats}>
                <Link className={styles.moretxbtn} to="/transactions">
                    <div>More Transactions</div> <ArrowRight />
                </Link>
            </TabFooter>
        );

    if (countQuery.isError) {
        return (
            <Banner variant="error" fullWidth>
                No transactions found.
            </Banner>
        );
    }

    if (transactionQuery.isError) {
        return (
            <Banner variant="error" fullWidth>
                There was an issue getting the latest transactions.
            </Banner>
        );
    }

    return (
        <div className={cl(styles.txlatestresults, styles[paginationtype])}>
            <TabGroup size="lg">
                <TabList>
                    <Tab>Transactions</Tab>
                </TabList>
                <TabPanels>
                    <TabPanel>
                        {recentTx ? (
                            <TableCard
                                refetching={transactionQuery.isPreviousData}
                                data={recentTx.data}
                                columns={recentTx.columns}
                            />
                        ) : (
                            <PlaceholderTable
                                rowCount={15}
                                rowHeight="16px"
                                colHeadings={[
                                    'Time',
                                    'Type',
                                    'Transaction ID',
                                    'Addresses',
                                    'Amount',
                                    'Gas',
                                ]}
                                colWidths={[
                                    '85px',
                                    '100px',
                                    '120px',
                                    '204px',
                                    '90px',
                                    '38px',
                                ]}
                            />
                        )}
                        {paginationtype !== 'none' &&
                            PaginationWithStatsOrStatsWithLink}
                    </TabPanel>
                </TabPanels>
            </TabGroup>
        </div>
    );
}
