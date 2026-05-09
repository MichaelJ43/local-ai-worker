# Build from repository root: docker build -f docs-site/Dockerfile .
# Layout must mirror the repo: csproj Content paths use ..\..\docs\... from docs-site/LocalAiWorker.Docs/.
FROM mcr.microsoft.com/dotnet/sdk:8.0 AS build
WORKDIR /workspace
COPY docs-site/ ./docs-site/
COPY docs/ ./docs/
WORKDIR /workspace/docs-site/LocalAiWorker.Docs
RUN dotnet publish -c Release -o /app /p:UseAppHost=false

FROM mcr.microsoft.com/dotnet/aspnet:8.0 AS runtime
WORKDIR /app
COPY --from=build /app .
ENV ASPNETCORE_URLS=http://0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["dotnet", "LocalAiWorker.Docs.dll"]
