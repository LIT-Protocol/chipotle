import "./globals.css";

export const metadata = {
  title: "Lit Solver Vault",
  description: "Policy-gated key custody for intent-system solvers — live ops view",
};

export default function RootLayout({ children }) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
