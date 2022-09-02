// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSubscriptionEvent } from '../index.guard';
import {
  SuiEventFilter,
  SuiEventEnvelope,
  SubscriptionId,
} from '../types';
import { Client as WsRpcClient} from 'rpc-websockets';


export const getWebsocketUrl = (httpUrl: string, port?: number): string => {
  const url = new URL(httpUrl);
  url.protocol = url.protocol.replace('http', 'ws');
  url.port = (port ?? 9001).toString();
  return url.toString();
};

enum ConnectionState {
  NotConnected,
  Connecting,
  Connected
}

type JsonRpcMethodMessage<T> = {
  jsonrpc: '2.0',
  method: string,
  params: T
}

type FilterSubHandler = {
  id: SubscriptionId,
  onMessage: (event: SuiEventEnvelope) => void,
  filter: SuiEventFilter
};

type SubscriptionData = {
  filter: SuiEventFilter,
  onMessage: (event: SuiEventEnvelope) => void
}

export type SubscriptionEvent = { subscription: SubscriptionId, result: SuiEventEnvelope };

export class WebsocketClient {
  protected wsClient: WsRpcClient;
  protected wsConnectionState: ConnectionState = ConnectionState.NotConnected;
  protected wsEndpoint: string;
  protected wsTimeout: number | null = null;
  protected wsSetup: boolean = false;

  protected activeSubscriptions: Map<SubscriptionId, SubscriptionData> = new Map();

  constructor(
    public endpoint: string,
    public skipValidation: boolean,
  ) {
    this.wsEndpoint = endpoint
    const socketOptions = { reconnect_interval: 3000, autoconnect: false };
    this.wsClient = new WsRpcClient(this.wsEndpoint, socketOptions);
  }

  private setupSocket() {
    if(this.wsSetup) return;

    this.wsClient.on('open', () => {
      if(this.wsTimeout) {
        clearTimeout(this.wsTimeout);
        this.wsTimeout = null;
      }
      this.wsConnectionState = ConnectionState.Connected;
      // underlying websocket is private, but we need it
      // to access messages sent by the node
      (this.wsClient as any).socket
        .on('message', this.onSocketMessage.bind(this));
    });

    this.wsClient.on('close', () => {
      this.wsConnectionState = ConnectionState.NotConnected;
    });

    this.wsClient.on('error', console.error);
    this.wsSetup = true;
  }

  // called for every message received from the node over websocket
  private onSocketMessage(rawMessage: string): void {
    const msg: JsonRpcMethodMessage<object> = JSON.parse(rawMessage);

    const params = msg.params;
    if(msg.method === 'sui_subscribeEvent' && isSubscriptionEvent(params)) {
      // call any registered handler for the message's subscription
      const sub = this.activeSubscriptions.get(params.subscription);
      if (sub)
        sub.onMessage(params.result);
    }
  }

  private async connect(): Promise<void> {
    if (this.wsConnectionState === ConnectionState.Connected)
      return Promise.resolve();

    this.setupSocket();
    this.wsClient.connect();
    this.wsConnectionState = ConnectionState.Connecting;

    return new Promise<void>((resolve, reject) => {
      this.wsTimeout = setTimeout(() => reject(new Error('timeout')), 15000) as any as number;
      this.wsClient.once('open', () => {
        this.refreshSubscriptions();
        resolve();
      });
      this.wsClient.once('error', reject);
    });
  }

    /**
    call only upon reconnecting to a node over websocket.
    calling multiple times on the same connection will result
    in multiple message handlers firing each time
  */
  private async refreshSubscriptions() {
    if(this.activeSubscriptions.size === 0)
      return;

    try {
      let newSubs: Map<SubscriptionId, SubscriptionData> = new Map();

      let newSubsArr: (FilterSubHandler | null)[] = await Promise.all(
        Array.from(this.activeSubscriptions.values())
        .map(async sub => {
          const onMessage = sub.onMessage;
          const filter = sub.filter;
          if(!filter || !onMessage)
            return Promise.resolve(null);
          /**
            re-subscribe to the same filter & replace the subscription id.
            we skip calling sui_unsubscribeEvent for the old sub id, because:
              * we assume this is being called after a reconnection
              * the node keys subscriptions with a combo of connection id & subscription id
          */
          const id = await this.subscribeEvent(filter, onMessage);
          return { id, onMessage, filter };
        })
      );

      newSubsArr.forEach(entry => {
        if(entry === null) return;
        const filter = entry.filter;
        const onMessage = entry.onMessage;
        newSubs.set(entry.id, { filter, onMessage });
      });

      this.activeSubscriptions = newSubs;
    } catch (err) {
      throw new Error(`error refreshing event subscriptions: ${err}`);
    }
  }

  async subscribeEvent(
    filter: SuiEventFilter,
    onMessage: (event: SuiEventEnvelope) => void
  ): Promise<SubscriptionId> {
    try {
      // lazily connect to websocket to avoid spamming node with connections
      if (this.wsConnectionState != ConnectionState.Connected)
        await this.connect();

      let subId = await this.wsClient.call(
        'sui_subscribeEvent',
        [filter],
        30000
      ) as SubscriptionId;

      this.activeSubscriptions.set(subId, { filter, onMessage });
      return subId;
    } catch (err) {
      throw new Error(
        `Error subscribing to event: ${err}, filter: ${JSON.stringify(filter)}`
      );
    }
  }

  async unsubscribeEvent(id: SubscriptionId): Promise<boolean> {
    try {
      if (this.wsConnectionState != ConnectionState.Connected)
        await this.connect();

      let removedOnNode = await this.wsClient.call(
        'sui_unsubscribeEvent',
        [id],
        30000
      ) as boolean;
      /**
        if the connection closes before unsubscribe is called,
        the remote node will remove us from its subscribers list without notification,
        leading to removedOnNode being false. but if we still had a record of it locally,
        we should still report that it was deleted successfully
      */
      return this.activeSubscriptions.delete(id) || removedOnNode;
    } catch (err) {
      throw new Error(
        `Error unsubscribing from event: ${err}, subscription: ${id}}`
      );
    }
  }
}