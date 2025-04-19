import React from "react";
import ReactDOM from "react-dom/client";
import Page from "./App";
import './index.css'; 

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Page />
  </React.StrictMode>,
);
