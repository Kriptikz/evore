"use client";

import { AdminProvider } from "@/context/AdminContext";

export default function AdminLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <AdminProvider>
      {children}
    </AdminProvider>
  );
}

