print("Telegram Bot Example starting...")

telegram.on_update(function(update)
    print("Received update: " .. json.encode(update))

    if update.message and update.message.text then
        local chat_id = update.message.chat.id
        local text = update.message.text

        print("Echoing message back to chat " .. chat_id)
        local ok, err = telegram.send_message(chat_id, "Echo: " .. text)
        if not ok then
            print("Failed to send message: " .. tostring(err))
        end
    end
end)

print("Handler registered. Waiting for updates...")
