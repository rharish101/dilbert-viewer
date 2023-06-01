// SPDX-FileCopyrightText: 2023 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

// Add keyboard shortcuts for navigating to links.
document.addEventListener("keyup", (ev) => {
  var linkId = null;

  switch (ev.code) {
    case "ArrowLeft":
      linkId = "prev-button";
      break;
    case "ArrowRight":
      linkId = "next-button";
      break;
  }

  if (linkId) {
    const link = document.getElementById(linkId);
    if (!link.classList.contains("disabled")) {
      window.location.href = link.getAttribute("href");
    }
  }
})
