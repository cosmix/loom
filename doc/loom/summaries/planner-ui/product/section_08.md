---
kind: document
of: doc/product/planner-ui/section_08.md
file: sha256:d403dfa34c0d4c990fbf08a5047742c32955c502fd7fcc6de1029b8fe38f8e89
cites:
  "doc/product/planner-ui/section_08.md#attachments": "sha256:756db7729a1b97d7581f99d873c770788dd49ec83c3b0721783d1bafca2c2ff6"
  "doc/product/planner-ui/section_08.md#composing-and-sending": "sha256:7a80f7feb3248c2476b414c2cc059e9c76a5e936532f17a79720f02f7ddfdaa5"
  "doc/product/planner-ui/section_08.md#connection-and-server-errors": "sha256:7dab9d65f15148edd3084cea2a8e8ca52b381e562c1567a2292d51893aeb17ee"
  "doc/product/planner-ui/section_08.md#ending-a-chat": "sha256:d1c1cb4774d5d6edaa4390a5fb10a5115ddc9d798fa568b2b24febe490e71a85"
  "doc/product/planner-ui/section_08.md#interrupt": "sha256:83b7c1a08ee5324b53dd2e4a3edcd364df071b9fa19bfc8ad868555b7baa769a"
  "doc/product/planner-ui/section_08.md#layout": "sha256:bc836394f5f91bc99d6e618743b0c2d0c82b71bae25ec1d6685c0a05559c796d"
  "doc/product/planner-ui/section_08.md#message-rendering": "sha256:3a14c569b5a19ee88585c482dd8456f911b0176daf731433e4bd491d8d7fb96f"
  "doc/product/planner-ui/section_08.md#permission-prompts": "sha256:0c28aefd354e4657fbec290258756edeb1e99fc2587616f01435150dfeebba30"
  "doc/product/planner-ui/section_08.md#pressure-runs": "sha256:62cc90c0a6734ffb693ec26068fda8c9e39c816488f77663c497cb19c5a8c935"
  "doc/product/planner-ui/section_08.md#queued-messages": "sha256:922fe1806e094e639859e7bf9269013d1dc5c4f6e93bec505a2938d5557ffc88"
  "doc/product/planner-ui/section_08.md#read-only-browsers": "sha256:4dec51b4636e684fb440d29bb3803c2d271b7dec3a51a68fedc87e43c415dcd3"
  "doc/product/planner-ui/section_08.md#session-states": "sha256:9a476cbe8e353ede3c8cbf9b51f45ce04ea83cb34fcc60f5f8352079c1986771"
  "doc/product/planner-ui/section_08.md#starting-a-chat": "sha256:0df6c17119a64ca26c2099c77f46a0fb816d4aad676f73a1fe36ed455e95624f"
  "doc/product/planner-ui/section_08.md#the-model-in-one-paragraph": "sha256:a7020d57616b511f478d5e77da816635d140ff2ed4cb50e303cd2b44aaa8e739"
  "doc/product/planner-ui/section_08.md#tool-activity": "sha256:521f79f2544e1c11de645f123c7a8228f6edfcd7a4ca0816c5150c236f10fe67"
  "doc/product/planner-ui/section_08.md#what-a-turn-changed": "sha256:a6d3f87485dea7be026b42844cc612d17ec1dfe2625e5357c3f34c0bf2a02647"
---

# 08 — Chat Interface

## Panel and turns

The chat panel shows one authoring conversation as turns, each with the user's message, the session's reply, and a footer of changes and cost. [The model](../../../../product/planner-ui/section_08.md#the-model-in-one-paragraph)

The panel is 26rem wide by default, resizes between 20rem and 40rem, and collapses to a 2.5rem rail. [Layout](../../../../product/planner-ui/section_08.md#layout)

## Rendering

Assistant text renders as Markdown without a bubble, and each transcript block appears whole as the session writes it. [Message rendering](../../../../product/planner-ui/section_08.md#message-rendering)

When a turn ends, three or more consecutive tool rows fold into one row that expands to the rows. [Tool activity](../../../../product/planner-ui/section_08.md#tool-activity)

## Session and permissions

The state line names each session state in words, such as Working, Permission, or Ended. [Session states](../../../../product/planner-ui/section_08.md#session-states)

A permission card appears in the transcript and stays pinned above the composer, with Allow and Deny for the holder only. [Permission prompts](../../../../product/planner-ui/section_08.md#permission-prompts)

## Sending and interrupts

Send queues a message while the session is Starting, Working, or waiting on a permission, and the composer refuses messages over the 100 KB cap. [Composing and sending](../../../../product/planner-ui/section_08.md#composing-and-sending) [Queued messages](../../../../product/planner-ui/section_08.md#queued-messages)

Stop interrupts the session and moves queued messages back into the composer instead of delivering them. [Interrupt](../../../../product/planner-ui/section_08.md#interrupt)

## Attachments and changes

Attachment chips show the passage's origin and first words, and a chip whose document changed shows "changed". [Attachments](../../../../product/planner-ui/section_08.md#attachments)

Each turn footer lists the changed files with Diff links and the rebuilt summaries with their token cost. [What a turn changed](../../../../product/planner-ui/section_08.md#what-a-turn-changed)

## Starting, ending, and runs

A chat starts from an offer, from New chat, or by sending a message when no chat runs. [Starting a chat](../../../../product/planner-ui/section_08.md#starting-a-chat)

End chat asks for confirmation, and closing the tab keeps the session for 10 minutes before it ends. [Ending a chat](../../../../product/planner-ui/section_08.md#ending-a-chat)

A pressure run occupies the panel under a run header, and Stop discards the run's progress after confirmation. [Pressure runs](../../../../product/planner-ui/section_08.md#pressure-runs)

## Read-only and errors

Read-only browsers see the same panel live, without the composer, Allow, Deny, or Stop. [Read-only browsers](../../../../product/planner-ui/section_08.md#read-only-browsers)

Connection failures show a banner, and the transcript stays readable, dimmed, with "Last update". [Connection and server errors](../../../../product/planner-ui/section_08.md#connection-and-server-errors)
