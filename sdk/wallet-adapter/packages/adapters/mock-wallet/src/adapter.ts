// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  Base64DataBuffer,
  MoveCallTransaction,
  SignaturePubkeyPair,
  SuiAddress,
  SuiTransactionResponse,
} from "@mysten/sui.js";
import { WalletAdapter } from "@mysten/wallet-adapter-base";

const ALL_PERMISSION_TYPES = ["viewAccount", "suggestTransactions"];
type AllPermissionsType = typeof ALL_PERMISSION_TYPES;
type PermissionType = AllPermissionsType[number];

interface SuiWallet {
  hasPermissions(permissions: readonly PermissionType[]): Promise<boolean>;
  requestPermissions(): Promise<boolean>;
  getAccounts(): Promise<SuiAddress[]>;
  signMessage(message: Base64DataBuffer): Promise<SignaturePubkeyPair>;
  executeMoveCall: (
    transaction: MoveCallTransaction
  ) => Promise<SuiTransactionResponse>;
  executeSerializedMoveCall: (
    transactionBytes: Uint8Array
  ) => Promise<SuiTransactionResponse>;
}
interface SuiWalletWindow {
  suiWallet: SuiWallet;
}

declare const window: SuiWalletWindow;

// Stored as state somewhere (Probably in a place with generics )
export class MockWalletAdapter implements WalletAdapter {
  connecting: boolean;
  connected: boolean;

  getAccounts(): Promise<string[]> {
    return window.suiWallet.getAccounts();
  }
  async signMessage(message: Base64DataBuffer): Promise<SignaturePubkeyPair> {
    return window.suiWallet.signMessage(message);
  }
  executeMoveCall(
    transaction: MoveCallTransaction
  ): Promise<SuiTransactionResponse> {
    return window.suiWallet.executeMoveCall(transaction);
  }
  executeSerializedMoveCall(
    transactionBytes: Uint8Array
  ): Promise<SuiTransactionResponse> {
    return window.suiWallet.executeSerializedMoveCall(transactionBytes);
  }

  name: string;

  async connect(): Promise<void> {
    this.connecting = true;
    if (window.suiWallet) {
      const wallet = window.suiWallet;
      try {
        let given = await wallet.requestPermissions();
        const newLocal: readonly PermissionType[] = ["viewAccount"];
        let perms = await wallet.hasPermissions(newLocal);
        console.log(perms);
        console.log(given);
        this.connected = true;
      } catch (err) {
        console.error(err);
      } finally {
        this.connecting = false;
      }
    }
  }

  // Come back to this later
  async disconnect(): Promise<void> {
    if (this.connected == true) {
      this.connected = false;
    }
    console.log("disconnected");
  }

  constructor(name: string) {
    this.connected = false;
    this.connecting = false;
    this.name = name;
  }
}
