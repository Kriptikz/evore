"use client";

import { Header } from "@/components/Header";
import Link from "next/link";

export default function PrivacyPolicyPage() {
  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-950 via-slate-900 to-slate-950">
      <Header />
      
      <main className="max-w-4xl mx-auto px-4 py-12">
        <div className="mb-8">
          <Link 
            href="/" 
            className="text-slate-400 hover:text-white transition-colors text-sm flex items-center gap-1 mb-4"
          >
            ← Back to Home
          </Link>
          <h1 className="text-3xl font-bold text-white mb-2">Privacy Policy</h1>
          <p className="text-slate-400">Last updated: December 29, 2024</p>
        </div>

        <div className="prose prose-invert prose-slate max-w-none space-y-8">
          <section className="bg-slate-800/30 border border-slate-700 rounded-xl p-6">
            <h2 className="text-xl font-semibold text-white mb-4">1. Introduction</h2>
            <p className="text-slate-300 leading-relaxed">
              Welcome to ORE Stats (&quot;we,&quot; &quot;our,&quot; or &quot;us&quot;). We are committed to protecting your privacy 
              and ensuring transparency about how we collect and use information. This Privacy Policy 
              explains our practices regarding data collection when you use our website and services.
            </p>
          </section>

          <section className="bg-slate-800/30 border border-slate-700 rounded-xl p-6">
            <h2 className="text-xl font-semibold text-white mb-4">2. Information We Collect</h2>
            <div className="space-y-4 text-slate-300">
              <div>
                <h3 className="text-lg font-medium text-slate-200 mb-2">2.1 Public Blockchain Data</h3>
                <p className="leading-relaxed">
                  We collect and display publicly available data from the Solana blockchain, including but not 
                  limited to wallet addresses, transaction histories, mining statistics, and on-chain program data. 
                  This information is publicly accessible on the blockchain and is not considered private data.
                </p>
              </div>
              <div>
                <h3 className="text-lg font-medium text-slate-200 mb-2">2.2 Wallet Connections</h3>
                <p className="leading-relaxed">
                  When you connect your wallet to our service, we receive your public wallet address. We do not 
                  have access to your private keys or seed phrases, and we cannot make transactions on your behalf 
                  without your explicit approval through your wallet.
                </p>
              </div>
              <div>
                <h3 className="text-lg font-medium text-slate-200 mb-2">2.3 Usage Data</h3>
                <p className="leading-relaxed">
                  We may collect anonymized usage data such as pages visited, time spent on the site, and 
                  general interaction patterns. This data is used solely to improve our services and user experience.
                </p>
              </div>
              <div>
                <h3 className="text-lg font-medium text-slate-200 mb-2">2.4 Server Logs</h3>
                <p className="leading-relaxed">
                  Our servers automatically collect certain information when you access our services, including 
                  IP addresses (which are hashed for privacy), browser type, and request timestamps. This data 
                  is used for security, performance monitoring, and rate limiting purposes.
                </p>
              </div>
            </div>
          </section>

          <section className="bg-slate-800/30 border border-slate-700 rounded-xl p-6">
            <h2 className="text-xl font-semibold text-white mb-4">3. How We Use Your Information</h2>
            <ul className="list-disc list-inside text-slate-300 space-y-2">
              <li>To provide and maintain our services</li>
              <li>To display mining statistics and leaderboard data</li>
              <li>To improve and optimize our platform</li>
              <li>To protect against abuse and ensure service availability</li>
              <li>To comply with legal obligations</li>
            </ul>
          </section>

          <section className="bg-slate-800/30 border border-slate-700 rounded-xl p-6">
            <h2 className="text-xl font-semibold text-white mb-4">4. Data Sharing</h2>
            <p className="text-slate-300 leading-relaxed">
              We do not sell, trade, or rent your personal information to third parties. We may share 
              anonymized, aggregated data for analytical purposes. Public blockchain data displayed on 
              our platform is inherently public and accessible to anyone.
            </p>
          </section>

          <section className="bg-slate-800/30 border border-slate-700 rounded-xl p-6">
            <h2 className="text-xl font-semibold text-white mb-4">5. Cookies and Tracking</h2>
            <p className="text-slate-300 leading-relaxed">
              We may use essential cookies to maintain session state and preferences. We do not use 
              third-party tracking cookies or advertising trackers. You can configure your browser 
              to refuse cookies, though this may affect some functionality.
            </p>
          </section>

          <section className="bg-slate-800/30 border border-slate-700 rounded-xl p-6">
            <h2 className="text-xl font-semibold text-white mb-4">6. Data Security</h2>
            <p className="text-slate-300 leading-relaxed">
              We implement appropriate technical and organizational measures to protect the data we 
              process. However, no method of transmission over the internet is 100% secure, and we 
              cannot guarantee absolute security.
            </p>
          </section>

          <section className="bg-slate-800/30 border border-slate-700 rounded-xl p-6">
            <h2 className="text-xl font-semibold text-white mb-4">7. Third-Party Services</h2>
            <p className="text-slate-300 leading-relaxed">
              Our service integrates with Solana blockchain RPC providers and wallet adapters. These 
              third-party services have their own privacy policies, and we encourage you to review them. 
              We are not responsible for the privacy practices of third-party services.
            </p>
          </section>

          <section className="bg-slate-800/30 border border-slate-700 rounded-xl p-6">
            <h2 className="text-xl font-semibold text-white mb-4">8. Your Rights</h2>
            <p className="text-slate-300 leading-relaxed">
              Depending on your jurisdiction, you may have rights regarding your personal data, including 
              the right to access, correct, or delete your information. Since we primarily display public 
              blockchain data, most information on our platform cannot be modified or deleted as it exists 
              on the immutable blockchain.
            </p>
          </section>

          <section className="bg-slate-800/30 border border-slate-700 rounded-xl p-6">
            <h2 className="text-xl font-semibold text-white mb-4">9. Children&apos;s Privacy</h2>
            <p className="text-slate-300 leading-relaxed">
              Our services are not directed to individuals under the age of 18. We do not knowingly 
              collect personal information from children. If you believe we have collected information 
              from a child, please contact us.
            </p>
          </section>

          <section className="bg-slate-800/30 border border-slate-700 rounded-xl p-6">
            <h2 className="text-xl font-semibold text-white mb-4">10. Changes to This Policy</h2>
            <p className="text-slate-300 leading-relaxed">
              We may update this Privacy Policy from time to time. We will notify users of any material 
              changes by updating the &quot;Last updated&quot; date at the top of this policy. Your continued use 
              of our services after changes constitutes acceptance of the updated policy.
            </p>
          </section>

          <section className="bg-slate-800/30 border border-slate-700 rounded-xl p-6">
            <h2 className="text-xl font-semibold text-white mb-4">11. Contact Us</h2>
            <p className="text-slate-300 leading-relaxed">
              If you have any questions about this Privacy Policy or our data practices, please reach 
              out to us through our official communication channels.
            </p>
          </section>
        </div>

        <div className="mt-12 pt-8 border-t border-slate-800">
          <p className="text-slate-500 text-sm text-center">
            © {new Date().getFullYear()} ORE Stats. All rights reserved.
          </p>
        </div>
      </main>
    </div>
  );
}

