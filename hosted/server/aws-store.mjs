import { DynamoDBClient } from "@aws-sdk/client-dynamodb";
import {
  DynamoDBDocumentClient,
  GetCommand,
  QueryCommand,
  TransactWriteCommand,
} from "@aws-sdk/lib-dynamodb";
import { Conflict } from "./service.mjs";

export function dynamoStore(table) {
  const client = DynamoDBDocumentClient.from(new DynamoDBClient({}), {
    marshallOptions: { removeUndefinedValues: true },
  });
  function condition(operation) {
    if (operation.absent)
      return { ConditionExpression: "attribute_not_exists(pk)" };
    if (!operation.matches) return {};
    const names = {},
      values = {},
      clauses = [];
    Object.entries(operation.matches).forEach(([name, value], index) => {
      names[`#c${index}`] = name;
      values[`:c${index}`] = value;
      clauses.push(`#c${index} = :c${index}`);
    });
    return {
      ConditionExpression: clauses.join(" AND "),
      ExpressionAttributeNames: names,
      ExpressionAttributeValues: values,
    };
  }
  return {
    async get(pk, sk) {
      return (
        await client.send(
          new GetCommand({
            TableName: table,
            Key: { pk, sk },
            ConsistentRead: true,
          }),
        )
      ).Item;
    },
    async list(pk, prefix, limit, cursor, reverse = false) {
      const reply = await client.send(
        new QueryCommand({
          TableName: table,
          KeyConditionExpression: "pk = :pk AND begins_with(sk, :prefix)",
          ExpressionAttributeValues: { ":pk": pk, ":prefix": prefix },
          Limit: limit,
          ScanIndexForward: !reverse,
          ConsistentRead: true,
          ...(cursor ? { ExclusiveStartKey: { pk, sk: cursor } } : {}),
        }),
      );
      return { items: reply.Items ?? [], cursor: reply.LastEvaluatedKey?.sk };
    },
    async commit(operations) {
      try {
        await client.send(
          new TransactWriteCommand({
            TransactItems: operations.map((operation) => {
              const common = { TableName: table, ...condition(operation) };
              if (operation.put)
                return { Put: { ...common, Item: operation.put } };
              if (operation.delete)
                return { Delete: { ...common, Key: operation.delete } };
              return { ConditionCheck: { ...common, Key: operation.check } };
            }),
          }),
        );
      } catch (error) {
        if (error.name === "TransactionCanceledException")
          throw new Conflict("Concurrent change. Retry.");
        throw error;
      }
    },
  };
}
