import re
def message_callback(message, metadata):
return re.sub(br'(?i)nof1(\.ai)?', b'', message)
