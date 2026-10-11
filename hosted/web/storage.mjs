// Only unfinished drafts belong here. Completed cloud history and tokens are not cached.
let database;
function open() {
  database ??= new Promise((resolve, reject) => {
    const request = indexedDB.open("mluva-everywhere-recovery", 1);
    request.onupgradeneeded = () => request.result.createObjectStore("drafts");
    request.onsuccess = () => resolve(request.result);
    request.onerror = () =>
      reject(
        new Error(
          "Device recovery storage is unavailable. Download your words before closing.",
        ),
      );
  });
  return database;
}
export async function draftStorage(owner, value) {
  const db = await open();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(
        "drafts",
        value === undefined ? "readonly" : "readwrite",
      ),
      drafts = tx.objectStore("drafts");
    const request =
      value === undefined
        ? drafts.get(owner)
        : value === null
          ? drafts.delete(owner)
          : drafts.put(value, owner);
    let result;
    request.onsuccess = () => {
      result = request.result;
    };
    tx.oncomplete = () => resolve(result);
    tx.onerror = () =>
      reject(
        new Error(
          "Local recovery could not be saved. Download your words before closing.",
        ),
      );
  });
}
