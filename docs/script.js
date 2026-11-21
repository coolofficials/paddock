"use strict";
const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)");
const previews = [...document.querySelectorAll("[data-preview]")];
function load(video) {
  const source = video.querySelector("source");
  if (!source.src) {
    source.src = source.dataset.src;
    video.load();
  }
}
function updateButton(video) {
  const button = video.nextElementSibling;
  const playing = !video.paused;
  button.textContent = playing ? "Pause" : "Play";
  button.setAttribute(
    "aria-label",
    (playing ? "Pause " : "Play ") + video.dataset.preview + " preview",
  );
  button.setAttribute("aria-pressed", String(playing));
}
function sync(video) {
  const allowed =
    video.dataset.userState === "playing" ||
    (!reducedMotion.matches && video.dataset.userState !== "paused");
  if (video.dataset.visible === "true" && !document.hidden && allowed) {
    load(video);
    video.play().catch(() => updateButton(video));
  } else {
    video.pause();
  }
  updateButton(video);
}
const observer = new IntersectionObserver(
  (entries) => {
    for (const entry of entries) {
      entry.target.dataset.visible = String(entry.isIntersecting);
      sync(entry.target);
    }
  },
  { threshold: 0.25 },
);
for (const video of previews) {
  video.muted = true;
  video.addEventListener("play", () => updateButton(video));
  video.addEventListener("pause", () => updateButton(video));
  video.nextElementSibling.addEventListener("click", () => {
    video.dataset.userState = video.paused ? "playing" : "paused";
    sync(video);
  });
  observer.observe(video);
  updateButton(video);
}
reducedMotion.addEventListener("change", () => {
  for (const video of previews) {
    delete video.dataset.userState;
    sync(video);
  }
});
document.addEventListener("visibilitychange", () => previews.forEach(sync));
