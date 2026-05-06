import React from "react";
import ReactDOM from "react-dom/client";
import { ErrorBoundary } from "react-error-boundary";
import App from "./App";
import "./styles/index.css";
import "./i18n";

function FallbackError({ error, resetErrorBoundary }: any) {
  return (
    <div className="flex min-h-screen flex-col items-center justify-center bg-gray-50 p-6 text-center text-ink">
      <h1 className="text-2xl font-bold text-red-600 mb-2">Aletheia Encountered a System Error</h1>
      <p className="max-w-xl text-gray-700 mb-6">{error.message}</p>
      <button 
        onClick={resetErrorBoundary}
        className="rounded bg-accent px-4 py-2 font-medium text-white transition hover:bg-accent-dark"
      >
        Restart Dashboard Shell
      </button>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <ErrorBoundary FallbackComponent={FallbackError}>
    <App />
  </ErrorBoundary>
);
