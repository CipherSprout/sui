// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  DEFAULT_SECP256R1_DERIVATION_PATH,
  PRIVATE_KEY_SIZE,
  Secp256r1Keypair,
} from '../../../src';
import { describe, it, expect } from 'vitest';
import { secp256r1 } from '@noble/curves/p256';
import { fromB64, toB64 } from '@mysten/bcs';
import { sha256 } from '@noble/hashes/sha256';

const VALID_SECP256R1_SECRET_KEY = [
  66, 37, 141, 205, 161, 76, 241, 17, 198, 2, 184, 151, 27, 140, 200, 67, 233,
  30, 70, 202, 144, 81, 81, 192, 39, 68, 166, 176, 23, 230, 147, 22,
];

// Corresponding to the secret key above.
export const VALID_SECP256R1_PUBLIC_KEY = [
  2, 39, 50, 43, 58, 137, 26, 10, 40, 13, 107, 193, 251, 44, 187, 35, 210, 143,
  84, 144, 111, 214, 64, 127, 95, 116, 31, 109, 239, 87, 98, 96, 154,
];

// Invalid private key with incorrect length
export const INVALID_SECP256R1_SECRET_KEY = Uint8Array.from(
  Array(PRIVATE_KEY_SIZE - 1).fill(1),
);

// Invalid public key with incorrect length
export const INVALID_SECP256R1_PUBLIC_KEY = Uint8Array.from(
  Array(PRIVATE_KEY_SIZE).fill(1),
);

// Test case generated against rust keytool cli. See https://github.com/MystenLabs/sui/blob/edd2cd31e0b05d336b1b03b6e79a67d8dd00d06b/crates/sui/src/unit_tests/keytool_tests.rs#L165
const TEST_CASES = [
  [
    'open genre century trouble allow pioneer love task chat salt drive income',
    'AgNbPsIqEtYdkvpBRIcgfxNev/J8Suohc3b3O5a5T/X7DA==',
    '0x5101291b764de08656e5b3fdf132d4cda20d604446681b166826bdb4996962e8',
  ],
];

const TEST_MNEMONIC =
  'open genre century trouble allow pioneer love task chat salt drive income';

describe('secp256r1-keypair', () => {
  it('new keypair', () => {
    const keypair = new Secp256r1Keypair();
    expect(keypair.getPublicKey().toBytes().length).toBe(33);
    expect(2).toEqual(2);
  });

  it('create keypair from secret key', () => {
    const secret_key = new Uint8Array(VALID_SECP256R1_SECRET_KEY);
    const pub_key = new Uint8Array(VALID_SECP256R1_PUBLIC_KEY);
    let pub_key_base64 = toB64(pub_key);
    const keypair = Secp256r1Keypair.fromSecretKey(secret_key);
    expect(keypair.getPublicKey().toBytes()).toEqual(new Uint8Array(pub_key));
    expect(keypair.getPublicKey().toBase64()).toEqual(pub_key_base64);
  });

  it('creating keypair from invalid secret key throws error', () => {
    const secret_key = new Uint8Array(INVALID_SECP256R1_SECRET_KEY);
    let secret_key_base64 = toB64(secret_key);
    const secretKey = fromB64(secret_key_base64);
    expect(() => {
      Secp256r1Keypair.fromSecretKey(secretKey);
    }).toThrow('private key must be 32 bytes, hex or bigint, not object');
  });

  it('generate keypair from random seed', () => {
    const keypair = Secp256r1Keypair.fromSeed(
      Uint8Array.from(Array(PRIVATE_KEY_SIZE).fill(8)),
    );
    expect(keypair.getPublicKey().toBase64()).toEqual(
      'AzrasV1mJWvxXNcWA1s/BBRE5RL+0d1k1Lp1WX0g42bx',
    );
  });

  it('signature of data is valid', async () => {
    const keypair = new Secp256r1Keypair();
    const signData = new TextEncoder().encode('hello world');

    const msgHash = sha256(signData);
    const sig = keypair.signData(signData);
    expect(
      secp256r1.verify(
        secp256r1.Signature.fromCompact(sig),
        msgHash,
        keypair.getPublicKey().toBytes(),
      ),
    ).toBeTruthy();
  });

  it('signature of data is same as rust implementation', async () => {
    const secret_key = new Uint8Array(VALID_SECP256R1_SECRET_KEY);
    const keypair = Secp256r1Keypair.fromSecretKey(secret_key);
    const signData = new TextEncoder().encode('Hello, world!');

    const msgHash = sha256(signData);
    const sig = keypair.signData(signData);

    // Assert the signature is the same as the rust implementation.
    expect(Buffer.from(sig).toString('hex')).toEqual(
      '26d84720652d8bc4ddd1986434a10b3b7b69f0e35a17c6a5987e6d1cba69652f4384a342487642df5e44592d304bea0ceb0fae2e347fa3cec5ce1a8144cfbbb2',
    );
    expect(
      secp256r1.verify(
        secp256r1.Signature.fromCompact(sig),
        msgHash,
        keypair.getPublicKey().toBytes(),
      ),
    ).toBeTruthy();
  });

  it('invalid mnemonics to derive secp256r1 keypair', () => {
    expect(() => {
      Secp256r1Keypair.deriveKeypair('aaa', DEFAULT_SECP256R1_DERIVATION_PATH);
    }).toThrow('Invalid mnemonic');
  });

  it('create keypair from secret key and mnemonics matches keytool', () => {
    for (const t of TEST_CASES) {
      // Keypair derived from mnemonic
      const keypair = Secp256r1Keypair.deriveKeypair(t[0]);
      expect(keypair.getPublicKey().toSuiAddress()).toEqual(t[2]);

      // Keypair derived from 32-byte secret key
      const raw = fromB64(t[1]);

      expect(raw.length).toEqual(PRIVATE_KEY_SIZE + 1);

      // The secp256r1 flag is 0x02. See more at [enum SignatureScheme].
      if (raw[0] !== 2 || raw.length !== PRIVATE_KEY_SIZE + 1) {
        throw new Error('invalid key');
      }
      const imported = Secp256r1Keypair.fromSecretKey(raw.slice(1));
      expect(imported.getPublicKey().toSuiAddress()).toEqual(t[2]);

      // Exported secret key matches the 32-byte secret key.
      const exported = imported.export();
      expect(exported.privateKey).toEqual(toB64(raw.slice(1)));
    }
  });

  it('incorrect purpose node for secp256r1 derivation path', () => {
    expect(() => {
      Secp256r1Keypair.deriveKeypair(TEST_MNEMONIC, `m/54'/784'/0'/0'/0'`);
    }).toThrow('Invalid derivation path');
  });

  it('incorrect hardened path for secp256k1 key derivation', () => {
    expect(() => {
      Secp256r1Keypair.deriveKeypair(TEST_MNEMONIC, `m/44'/784'/0'/0'/0'`);
    }).toThrow('Invalid derivation path');
  });
});
