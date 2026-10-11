import { finishSignIn } from "./auth.mjs";
try {
  await finishSignIn(
    await (await fetch("/config.json", { cache: "no-store" })).json(),
  );
  location.replace("/");
} catch (error) {
  document.getElementById("callback-status").textContent = error.message;
}
