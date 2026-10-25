import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Overlay from "./Overlay";
import "./styles.css";

const isOverlay = window.location.hash.includes("overlay");
if (isOverlay) document.body.classList.add("overlay-body");

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>{isOverlay ? <Overlay /> : <App />}</React.StrictMode>
);
