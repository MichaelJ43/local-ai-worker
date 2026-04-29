import { test, expect } from "@playwright/test";

test("home renders app title in shell", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator(".brand strong")).toHaveText("Local AI Worker");
});

test("workers view opens from nav and shows hybrid panel", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Workers" }).click();
  await expect(page.getByRole("heading", { name: "Workers" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Add worker" })).toBeVisible();
  await page.getByRole("button", { name: "Add worker" }).click();
  await expect(page.getByTestId("worker-hybrid-section")).toBeVisible();
  await expect(page.getByRole("button", { name: "Local attempt + escalate" })).toBeVisible();
});
