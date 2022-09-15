// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { hasPublicTransfer } from '@mysten/sui.js';
import { useMemo } from 'react';
import { Link } from 'react-router-dom';

import { Content } from '_app/shared/bottom-menu-layout';
import PageTitle from '_app/shared/page-title';
import NFTdisplay from '_components/nft-display';
import { useAppSelector } from '_hooks';
import { accountNftsSelector } from '_redux/slices/account';
import { trackPageview } from '_shared/constants';

import st from './NFTPage.module.scss';

function NftsPage() {
    const nfts = useAppSelector(accountNftsSelector);
    const filteredNfts = useMemo(
        () => nfts.filter((nft) => hasPublicTransfer(nft)),
        [nfts]
    );

    trackPageview();
    return (
        <div className={st.container}>
            <PageTitle
                title="NFTs"
                stats={`${filteredNfts.length}`}
                className={st.pageTitle}
            />
            <Content>
                <section className={st.nftGalleryContainer}>
                    <section className={st.nftGallery}>
                        {filteredNfts.map((nft) => (
                            <Link
                                to={`/nft-details?${new URLSearchParams({
                                    objectId: nft.reference.objectId,
                                }).toString()}`}
                                key={nft.reference.objectId}
                            >
                                <NFTdisplay nftobj={nft} showlabel={true} />
                            </Link>
                        ))}
                    </section>
                </section>
            </Content>
        </div>
    );
}

export default NftsPage;
