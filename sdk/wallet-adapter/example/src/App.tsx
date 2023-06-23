// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import './App.css';
import { ConnectButton, useWalletKit } from '@mysten/wallet-kit';
import { TransactionBlock } from '@mysten/sui.js';
import { useEffect } from 'react';
import { QredoConnectButton } from './QredoConnectButton';

function App() {
	const {
		currentWallet,
		currentAccount,
		signTransactionBlock,
		signAndExecuteTransactionBlock,
		signMessage,
	} = useWalletKit();

	useEffect(() => {
		// You can do something with `currentWallet` here.
	}, [currentWallet]);

	return (
		<div className="App">
			<ConnectButton />
			<div>
				<button
					onClick={async () => {
						const txb = new TransactionBlock();
						const [coin] = txb.splitCoins(txb.gas, [txb.pure(1)]);
						txb.transferObjects([coin], txb.pure(currentAccount!.address));

						console.log(await signTransactionBlock({ transactionBlock: txb }));
					}}
				>
					Sign Transaction
				</button>
			</div>
			<div>
				<button
					onClick={async () => {
						const txb = new TransactionBlock();
						const [coin] = txb.splitCoins(txb.gas, [txb.pure(1)]);
						txb.transferObjects([coin], txb.pure(currentAccount!.address));

						console.log(
							await signAndExecuteTransactionBlock({
								transactionBlock: txb,
								options: { showEffects: true },
							}),
						);
					}}
				>
					Sign + Execute Transaction
				</button>
			</div>
			<div>
				<button
			        	onClick={async () => {
			        		if (currentAccount) {
			        			let message = new TextEncoder().encode("hello world");
			        			signMessage({ message: message })
			        				.then(async (res: SignedMessage) => {
			        					const signature = fromSerializedSignature(res.signature);
			        					let pubKey = new Ed25519PublicKey(signature.pubKey.data);
									console.log(
										"address from signature: ",
										pubKey.toSuiAddress()
									);
								})
								.catch((e: Error) => {
								  console.log("user rejected signing message");
								});
								}
							console.log(
						    		"Verify Signed Message:",
						        	await verifyMessage(
							        	new TextEncoder().encode("hello world"),
							        	res.signature,
							        	3
						        	)
          );
			        }}
			        >
			          Sign message
			        </button>
			</div>
			<hr />
			<div>
				<h3>Qredo Connect</h3>
				<QredoConnectButton />
			</div>
		</div>
	);
}

export default App;
