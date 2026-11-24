export type AppEnvironment = "development" | "production";

export const appEnvironment: AppEnvironment =
  import.meta.env.VITE_APP_ENV === "production" ? "production" : "development";
