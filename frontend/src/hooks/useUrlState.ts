"use client";

import { useCallback, useEffect, useState, useRef, useMemo } from "react";
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
  
  // Store default value in ref to avoid dependency changes
  const defaultRef = useRef(defaultValue);

  // Parse value from URL
  const parseValue = useCallback((): T => {
    const urlValue = searchParams.get(key);
    if (urlValue === null) {
      return defaultRef.current;
    }

    // Parse based on type of default value
    if (typeof defaultRef.current === "number") {
      const parsed = Number(urlValue);
      return (isNaN(parsed) ? defaultRef.current : parsed) as T;
    }
    if (typeof defaultRef.current === "boolean") {
      return (urlValue === "true") as T;
    }
    return urlValue as T;
  }, [searchParams, key]);

  const [value, setValue] = useState<T>(() => parseValue());

  // Sync state when URL changes (browser back/forward)
  useEffect(() => {
    setValue(parseValue());
  }, [parseValue]);

  // Update URL when value changes
  const updateValue = useCallback((newValue: T) => {
    setValue(newValue);

    const params = new URLSearchParams(searchParams.toString());
    
    if (newValue === defaultRef.current || newValue === undefined || newValue === "") {
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
  }, [router, pathname, searchParams, key, replace]);

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

  // Memoize defaults to avoid reference changes
  const defaultsRef = useRef(defaults);
  const defaultKeys = useMemo(() => Object.keys(defaults), []);

  // Parse all values from URL - only run once on mount and when searchParams change
  const parseValues = useCallback((): T => {
    const result = { ...defaultsRef.current };
    
    for (const key of defaultKeys) {
      const urlValue = searchParams.get(key);
      if (urlValue !== null) {
        const defaultValue = defaultsRef.current[key];
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
  }, [searchParams, defaultKeys]);

  const [values, setValues] = useState<T>(() => parseValues());

  // Sync state when URL changes
  useEffect(() => {
    setValues(parseValues());
  }, [parseValues]);

  // Update multiple URL params at once
  const updateValues = useCallback((updates: Partial<T>) => {
    // When setting values, use the default if the new value is undefined
    setValues(prev => {
      const next = { ...prev };
      for (const [key, newValue] of Object.entries(updates)) {
        if (newValue === undefined) {
          // Use the default value instead of undefined
          (next as Record<string, unknown>)[key] = defaultsRef.current[key];
        } else {
          (next as Record<string, unknown>)[key] = newValue;
        }
      }
      return next;
    });

    const params = new URLSearchParams(searchParams.toString());
    
    for (const [key, newValue] of Object.entries(updates)) {
      const defaultValue = defaultsRef.current[key];
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
  }, [router, pathname, searchParams, replace]);

  return [values, updateValues];
}
