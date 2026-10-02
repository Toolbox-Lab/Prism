import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "Grat — Soroban Transaction Debugger",
  description:
    "From cryptic error to root cause in one command. Decode, trace, and debug Soroban transactions.",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
