import { Conflict } from "../server/service.mjs";
export function memoryStore() {
  const rows = new Map(),
    id = (pk, sk) => `${pk}\0${sk}`;
  return {
    rows,
    async get(pk, sk) {
      return structuredClone(rows.get(id(pk, sk)));
    },
    async list(pk, prefix, limit, cursor, reverse = false) {
      let items = [...rows.values()]
        .filter((r) => r.pk === pk && r.sk.startsWith(prefix))
        .sort((a, b) => a.sk.localeCompare(b.sk));
      if (reverse) items.reverse();
      if (cursor)
        items = items.filter((r) => (reverse ? r.sk < cursor : r.sk > cursor));
      return {
        items: structuredClone(items.slice(0, limit)),
        cursor: items.length > limit ? items[limit - 1].sk : undefined,
      };
    },
    async commit(operations) {
      for (const op of operations) {
        const { pk, sk } = op.put ?? op.delete ?? op.check,
          current = rows.get(id(pk, sk));
        if (op.absent && current) throw new Conflict();
        if (
          op.matches &&
          (!current ||
            Object.entries(op.matches).some(([k, v]) => current[k] !== v))
        )
          throw new Conflict();
      }
      for (const op of operations) {
        if (op.put) rows.set(id(op.put.pk, op.put.sk), structuredClone(op.put));
        else if (op.delete) rows.delete(id(op.delete.pk, op.delete.sk));
      }
    },
  };
}
