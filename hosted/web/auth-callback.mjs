import { finishSignIn } from "./auth.mjs";
try {
  await finishSignIn(
    await (await fetch("/config.json", { cache: "no-store" })).json(),
  );
  location.replace("/");
} catch (error) {
  document.body.classList.add("callback-failed");
  document.getElementById("callback-title").textContent =
    "Sign-in didn’t finish";
  document.getElementById("callback-status").textContent = error.message;
}
