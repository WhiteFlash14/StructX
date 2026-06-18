import { IBM_Plex_Mono, Inter } from "next/font/google";

import { Providers } from "./providers";

const inter = Inter({
  subsets: ["latin"],
  display: "swap",
  variable: "--font-sans",
});

const plexMono = IBM_Plex_Mono({
  subsets: ["latin"],
  weight: ["400", "500", "600"],
  display: "swap",
  variable: "--font-plex-mono",
});

export const metadata = {
  title: "StructX",
  description: "Struct X, powered by DeepBook Predict",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    // suppressHydrationWarning on <html> and <body> only suppresses the
    // *one-level-deep* attribute diff that browser extensions like Dark
    // Reader inject (data-darkreader-* on <html> and on inline styles/SVGs).
    // It does NOT silence real hydration mismatches inside our components.
    <html
      lang="en"
      className={`${inter.variable} ${plexMono.variable}`}
      suppressHydrationWarning
    >
      <body className={inter.className} suppressHydrationWarning>
        <Providers>{children}</Providers>
      </body>
    </html>
  );
}
