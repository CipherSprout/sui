import {
  useCurrentAccount,
  useSignAndExecuteTransactionBlock,
  useSuiClientQuery,
} from "@mysten/dapp-kit";
import { SuiObjectData } from "@mysten/sui.js/client";
import { TransactionBlock } from "@mysten/sui.js/transactions";
import { Button, Flex, Heading, Text } from "@radix-ui/themes";
import { useQueryClient } from "@tanstack/react-query";
import { PACKAGE_ID } from "./constants";

export function Counter({ id }: { id: string }) {
  const currentAccount = useCurrentAccount();
  const queryClient = useQueryClient();
  const { mutate: signAndExecute } = useSignAndExecuteTransactionBlock();
  const queryKey = ["getObject", id];
  const { data, isLoading, error } = useSuiClientQuery(
    "getObject",
    {
      id,
      options: {
        showContent: true,
        showOwner: true,
      },
    },
    {
      queryKey,
    },
  );

  const executeMoveCall = (method: "increment" | "reset") => {
    const txb = new TransactionBlock();

    if (method === "reset") {
      txb.moveCall({
        arguments: [txb.object(id), txb.pure.u64(0)],
        target: `${PACKAGE_ID}::counter::set_value`,
      });
    } else {
      txb.moveCall({
        arguments: [txb.object(id)],
        target: `${PACKAGE_ID}::counter::increment`,
      });
    }

    signAndExecute(
      {
        requestType: "WaitForEffectsCert",
        transactionBlock: txb,
        options: {
          showEffects: true,
          showObjectChanges: true,
        },
      },
      {
        onSuccess: () => queryClient.invalidateQueries(queryKey),
      },
    );
  };

  if (isLoading) return <Text>Loading...</Text>;

  if (error) return <Text>Error: {error.message}</Text>;

  if (!data.data) return <Text>Not found</Text>;

  const ownedByCurrentAccount = getOwner(data.data) === currentAccount?.address;

  return (
    <>
      <Heading size="3">Counter {id}</Heading>

      <Flex direction="column" gap="2">
        <Text>Count: {getCount(data.data!)}</Text>
        <Flex direction="row" gap="2">
          <Button onClick={() => executeMoveCall("increment")}>
            Increment
          </Button>
          {ownedByCurrentAccount ? (
            <Button onClick={() => executeMoveCall("reset")}>Reset</Button>
          ) : null}
        </Flex>
      </Flex>
    </>
  );
}

function getCount(data: SuiObjectData) {
  if (data.content?.dataType !== "moveObject") {
    return 0;
  }

  return (data.content.fields as Record<string, number>).value;
}

function getOwner(data: SuiObjectData) {
  if (data.content?.dataType !== "moveObject") {
    return null;
  }

  return (data.content.fields as Record<string, string>).owner;
}
