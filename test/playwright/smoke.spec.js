import { test, expect } from "@playwright/test";

test("home renders main heading", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Local AI Worker" })).toBeVisible();
});

test("workers section is present", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Workers" })).toBeVisible();
});
