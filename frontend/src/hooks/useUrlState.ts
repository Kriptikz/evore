"use client";

import { useCallback, useEffect, useState } from "react";
import { useRouter, useSearchParams, usePathname } from "next/navigation";

/**
 * Hook for syncing state with URL query parameters.
 * Supports browser back/forward navigation.
 * 
 * @param key - The query parameter key
 * @param defaultValue - Default value when param is not in URL
 * @param options - Optional configuration
 */
export function useUrlState<T extends string | number | boolean | undefined>(
  key: string,
  defaultValue: T,
  options?: {
    /** If true, replaces history entry instead of pushing */
    replace?: boolean;
  }
): [T, (value: T) => void] {
  const router = useRouter();
  const pathname = usePathname();
  const searchParams = useSearchParams();
  const replace = options?.replace ?? false;

  // Parse value from URL
  const parseValue = useCallback((): T => {
    const urlValue = searchParams.get(key);
    if (urlValue === null) {
      return defaultValue;
    }

    // Parse based on type of default value
    if (typeof defaultValue === "number") {
      const parsed = Number(urlValue);
      return (isNaN(parsed) ? defaultValue : parsed) as T;
    }
    if (typeof defaultValue === "boolean") {
      return (urlValue === "true") as T;
    }
    return urlValue as T;
  }, [searchParams, key, defaultValue]);

  const [value, setValue] = useState<T>(parseValue);

  // Sync state when URL changes (browser back/forward)
  useEffect(() => {
    setValue(parseValue());
  }, [parseValue]);

  // Update URL when value changes
  const updateValue = useCallback((newValue: T) => {
    setValue(newValue);

    const params = new URLSearchParams(searchParams.toString());
    
    if (newValue === defaultValue || newValue === undefined || newValue === "") {
      // Remove param if it's the default value
      params.delete(key);
    } else {
      params.set(key, String(newValue));
    }

    const queryString = params.toString();
    const newUrl = queryString ? `${pathname}?${queryString}` : pathname;

    if (replace) {
      router.replace(newUrl, { scroll: false });
    } else {
      router.push(newUrl, { scroll: false });
    }
  }, [router, pathname, searchParams, key, defaultValue, replace]);

  return [value, updateValue];
}

/**
 * Hook for managing multiple URL state values at once.
 * More efficient than multiple useUrlState calls.
 */
export function useMultiUrlState<T extends Record<string, string | number | boolean | undefined>>(
  defaults: T,
  options?: {
    replace?: boolean;
  }
): [T, (updates: Partial<T>) => void] {
  const router = useRouter();
  const pathname = usePathname();
  const searchParams = useSearchParams();
  const replace = options?.replace ?? false;

  // Parse all values from URL
  const parseValues = useCallback((): T => {
    const result = { ...defaults };
    
    for (const key of Object.keys(defaults)) {
      const urlValue = searchParams.get(key);
      if (urlValue !== null) {
        const defaultValue = defaults[key];
        if (typeof defaultValue === "number") {
          const parsed = Number(urlValue);
          (result as Record<string, unknown>)[key] = isNaN(parsed) ? defaultValue : parsed;
        } else if (typeof defaultValue === "boolean") {
          (result as Record<string, unknown>)[key] = urlValue === "true";
        } else {
          (result as Record<string, unknown>)[key] = urlValue;
        }
      }
    }
    
    return result;
  }, [searchParams, defaults]);

  const [values, setValues] = useState<T>(parseValues);

  // Sync state when URL changes
  useEffect(() => {
    setValues(parseValues());
  }, [parseValues]);

  // Update multiple URL params at once
  const updateValues = useCallback((updates: Partial<T>) => {
    const newValues = { ...values, ...updates };
    setValues(newValues);

    const params = new URLSearchParams(searchParams.toString());
    
    for (const [key, newValue] of Object.entries(updates)) {
      const defaultValue = defaults[key];
      if (newValue === defaultValue || newValue === undefined || newValue === "") {
        params.delete(key);
      } else {
        params.set(key, String(newValue));
      }
    }

    const queryString = params.toString();
    const newUrl = queryString ? `${pathname}?${queryString}` : pathname;

    if (replace) {
      router.replace(newUrl, { scroll: false });
    } else {
      router.push(newUrl, { scroll: false });
    }
  }, [router, pathname, searchParams, values, defaults, replace]);

  return [values, updateValues];
}

