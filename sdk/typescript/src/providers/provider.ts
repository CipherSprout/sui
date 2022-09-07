// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SignatureScheme } from '../cryptography/publickey';
import {
  GetObjectDataResponse,
  SuiObjectInfo,
  GatewayTxSeqNumber,
  GetTxnDigestsResponse,
  SuiTransactionResponse,
  SuiObjectRef,
  SuiMoveFunctionArgTypes,
  SuiMoveNormalizedFunction,
  SuiMoveNormalizedStruct,
  SuiMoveNormalizedModule,
  SuiMoveNormalizedModules,
  SuiEventFilter,
  SuiEventEnvelope,
  SubscriptionId,
  ExecuteTransactionRequestType,
  SuiExecuteTransactionResponse,
  TransactionDigest,
  ObjectId,
  TimeRangeQueryOptions,
  SuiAddress,
  ObjectOwner,
  SuiEvents,
} from '../types';

///////////////////////////////
// Exported Abstracts
export abstract class Provider {
  // Objects
  /**
   * Get all objects owned by an address
   */
  abstract getObjectsOwnedByAddress(
    addressOrObjectId: string
  ): Promise<SuiObjectInfo[]>;

  /**
   * Convenience method for getting all gas objects(SUI Tokens) owned by an address
   */
  abstract getGasObjectsOwnedByAddress(
    _address: string
  ): Promise<SuiObjectInfo[]>;

  /**
   * Get details about an object
   */
  abstract getObject(objectId: string): Promise<GetObjectDataResponse>;

  /**
   * Get object reference(id, tx digest, version id)
   * @param objectId
   */
  abstract getObjectRef(objectId: string): Promise<SuiObjectRef | undefined>;

  // Transactions
  /**
   * Get transaction digests for a given range
   *
   * NOTE: this method may get deprecated after DevNet
   */
  abstract getTransactionDigestsInRange(
    start: GatewayTxSeqNumber,
    end: GatewayTxSeqNumber
  ): Promise<GetTxnDigestsResponse>;

  /**
   * Get the latest `count` transactions
   *
   * NOTE: this method may get deprecated after DevNet
   */
  abstract getRecentTransactions(count: number): Promise<GetTxnDigestsResponse>;

  /**
   * Get total number of transactions
   * NOTE: this method may get deprecated after DevNet
   */
  abstract getTotalTransactionNumber(): Promise<number>;

  abstract executeTransaction(
    txnBytes: string,
    signatureScheme: SignatureScheme,
    signature: string,
    pubkey: string
  ): Promise<SuiTransactionResponse>;

  /**
   * This is under development endpoint on Fullnode that will eventually
   * replace the other `executeTransaction` that's only available on the
   * Gateway
   */
  abstract executeTransactionWithRequestType(
    txnBytes: string,
    signatureScheme: SignatureScheme,
    signature: string,
    pubkey: string,
    requestType: ExecuteTransactionRequestType
  ): Promise<SuiExecuteTransactionResponse>;

  // Move info
  /**
   * Get Move function argument types like read, write and full access
   */
  abstract getMoveFunctionArgTypes(
    objectId: string,
    moduleName: string,
    functionName: string
  ): Promise<SuiMoveFunctionArgTypes>;

  /**
   * Get a map from module name to
   * structured representations of Move modules
   */
  abstract getNormalizedMoveModulesByPackage(
    objectId: string
  ): Promise<SuiMoveNormalizedModules>;

  /**
   * Get a structured representation of Move module
   */
  abstract getNormalizedMoveModule(
    objectId: string,
    moduleName: string
  ): Promise<SuiMoveNormalizedModule>;

  /**
   * Get a structured representation of Move function
   */
  abstract getNormalizedMoveFunction(
    objectId: string,
    moduleName: string,
    functionName: string
  ): Promise<SuiMoveNormalizedFunction>;

  /**
   * Get a structured representation of Move struct
   */
  abstract getNormalizedMoveStruct(
    objectId: string,
    moduleName: string,
    structName: string
  ): Promise<SuiMoveNormalizedStruct>;

  abstract syncAccountState(address: string): Promise<any>;

  /**
   * Get events for one transaction
   * @param digest transaction digest to search by
   * @param count max result count
   */
  abstract getEventsByTransaction(digest: TransactionDigest, count: number): Promise<SuiEvents>;

  /**
   * Get events emitted from within the specified Move module
   * @param package_ Move package object ID
   * @param module Move module name
   * @param options.count max result count
   * @param options.startTime start of time range
   * @param options.endTime end of time range
   */
  abstract getEventsByTransactionModule(
    package_: ObjectId,                   // 'package' is reserved word
    module: string,
    options: TimeRangeQueryOptions
  ): Promise<SuiEvents>;

  /**
   * Get events with a matching Move type name
   * @param moveEventStructName Move struct type name
   * @param options.count max result count
   * @param options.startTime start of time range to search
   * @param options.endTime end of time range
   */
  abstract getEventsByMoveEventStructName(
    moveEventStructName: string,
    options: TimeRangeQueryOptions
  ): Promise<SuiEvents>;

  /**
   * Get events with a matching Move type name
   * @param sender Sui address of the sender of the transaction that generated the event
   * @param options.count max result count
   * @param options.startTime start of time range to search
   * @param options.endTime end of time range
   */
  abstract getEventsBySender(
    sender: SuiAddress,
    options: TimeRangeQueryOptions
  ): Promise<SuiEvents>;

  /**
   * Get events with a matching recipient
   * @param recipient object owner that received the transaction that generated the event
   * @param options.count max result count
   * @param options.startTime start of time range to search
   * @param options.endTime end of time range
   */
  abstract getEventsByRecipient(
    recipient: ObjectOwner,
    options: TimeRangeQueryOptions
  ): Promise<SuiEvents>;

  /**
   * Get events involving the given object
   * @param object object id created, mutated, or deleted in events
   * @param options.count max result count
   * @param options.startTime start of time range to search
   * @param options.endTime end of time range
   */
  abstract getEventsByObject(
    object: ObjectId,
    options: TimeRangeQueryOptions
  ): Promise<SuiEvents>;

  /**
   * Get all events within the given time span
   * @param options.count max result count
   * @param options.startTime start of time range to search
   * @param options.endTime end of time range
   */
  abstract getEventsByTimeRange(options: TimeRangeQueryOptions): Promise<SuiEvents>;

  /**
   * Subscribe to get notifications whenever an event matching the filter occurs
   * @param filter filter describing the subset of events to follow
   * @param onMessage function to run when we receive a notification of a new event matching the filter
   */
  abstract subscribeEvent(
    filter: SuiEventFilter,
    onMessage: (event: SuiEventEnvelope) => void
  ): Promise<SubscriptionId>;

  /**
   * Unsubscribe from an event subscription
   * @param id - subscription id to unsubscribe from (previously received from subscribeEvent)
   */
  abstract unsubscribeEvent(id: SubscriptionId): Promise<boolean>;
  // TODO: add more interface methods
}
