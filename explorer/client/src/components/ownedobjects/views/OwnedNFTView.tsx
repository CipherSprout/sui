// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { trimStdLibPrefix, alttextgen } from '../../../utils/stringUtils';
import DisplayBox from '../../displaybox/DisplayBox';
import Longtext from '../../longtext/Longtext';
import { type DataType } from '../OwnedObjectConstants';

import styles from '../styles/OwnedObjects.module.css';

export default function OwnedNFTView({ results }: { results: DataType }) {
    const lastRowHas2Elements = (itemList: any[]): boolean =>
        itemList.length % 3 === 2;

    return (
        <div id="ownedObjects" className={styles.ownedobjects}>
            {results.map((entryObj, index1) => (
                <div className={styles.objectbox} key={`object-${index1}`}>
                    {entryObj.display !== undefined && (
                        <div className={styles.previewimage}>
                            <DisplayBox display={entryObj.display} />
                        </div>
                    )}
                    <div className={styles.textitem}>
                        <div>
                            <Longtext
                                text={entryObj.id}
                                category="objects"
                                isCopyButton={false}
                                alttext={alttextgen(entryObj.id)}
                            />
                        </div>
                        <div>
                            <span className={styles.typevalue}>
                                {trimStdLibPrefix(entryObj.Type)}
                            </span>
                        </div>
                    </div>
                </div>
            ))}
            {lastRowHas2Elements(results) && (
                <div className={`${styles.objectbox} ${styles.fillerbox}`} />
            )}
        </div>
    );
}
