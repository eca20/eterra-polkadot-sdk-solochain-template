# Eterra Pallet Integration Guide

This guide provides a detailed walkthrough for integrating the **Eterra pallet** with a Unity game using the Substrate.NET API. It enables seamless interaction with the blockchain for managing and synchronizing gameplay mechanics.

## Overview

The **Eterra pallet** allows for decentralized gameplay interactions using the Substrate framework. Key functionalities include:

- **Game Creation**: Start a new game on-chain.
- **Turn-Based Actions**: Submit moves and update game states.
- **Score Management**: Track and synchronize scores for players.

This guide outlines how to integrate these features using the **Substrate.NET API** in Unity.

---

## Prerequisites

1. **Unity**: Installed Unity editor.
2. **Substrate.NET API**: Library for interacting with Substrate-based blockchains.
3. **EterraSDK**: A custom SDK for accessing the Eterra pallet.
4. **Local Node**: A Substrate-based node running the Eterra pallet.

---

## Integration Steps

### 1. Add Dependencies

Include the following libraries in your Unity project:
- `Substrate.NetApi`
- `EterraSDK`

### 2. Initialize Substrate Network Client

Set up a connection to your Substrate-based blockchain in Unity.

#### Code Example:
```csharp
private static SubstrateNetwork InitializeClient(string nodeUrl)
{
    var client = new SubstrateNetwork(EterraSDK.NetApiExt.Client.BaseClient.Alice, nodeUrl);

    client.ExtrinsicManager.ExtrinsicUpdated += (id, info) =>
    {
        Debug.Log("ExtrinsicUpdated: " + id + " | " + info.TransactionEvent);
    };

    ConnectClientAsync(client, nodeUrl);
    return client;
}

private static async Task ConnectClientAsync(SubstrateNetwork client, string nodeUrl)
{
    await client.ConnectAsync(true, true, CancellationToken.None);
    Debug.Log("Connected to " + nodeUrl + " = " + client.IsConnected);
}
```

##### Create Game
```csharp
public static async Task CreateGame(SubstrateNetwork client, Account creator, Account opponent)
{
    var gameId = await client.CallMethodAsync<GameId>(
        "eterra.createGame",
        new MultiAddress(creator),
        new MultiAddress(opponent)
    );

    Debug.Log("Game created with ID: " + gameId);
}
```

##### Submit Turn
```csharp
public static async Task SubmitTurn(SubstrateNetwork client, Account player, GameId gameId, int x, int y, Card card)
{
    var result = await client.CallMethodAsync<bool>(
        "eterra.playTurn",
        new MultiAddress(player),
        gameId,
        x,
        y,
        card
    );

    Debug.Log("Turn submitted: " + result);
}
```

##### Fetch Game State
```csharp
public static async Task<GameState> GetGameState(SubstrateNetwork client, GameId gameId)
{
    var gameState = await client.QueryStorageAsync<GameState>(
        "Eterra.GameBoard",
        gameId
    );

    Debug.Log("Game state fetched: " + gameState);
    return gameState;
}
```