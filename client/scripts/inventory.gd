# Inventar (#18): Das Terrain hält den letzten Stand vom Server (`get_inventory`); diese Anzeige
# zeigt unten das gewählte Baumaterial und mit I die ganze Liste mit den Werten der Stapel.
#
# Die Plätze haben feste Nummern (leere bleiben leer). 1-9 wählen Platz 1-9, das Mausrad springt
# zwischen belegten Plätzen, im offenen Inventar wählt ein Klick. Solange es offen ist, gräbt ein
# Klick nicht.
extends CanvasLayer

var terrain: TerraforgeTerrain
var player: Node3D
var stacks: Array = []
## Was die Liste zuletzt zeigte (nur bei Änderungen neu aufbauen, sonst springt sie beim Scrollen).
var shown := ""
var refresh := 0.0
var hotbar: Label
var panel: PanelContainer
var list: ItemList
var details: Label


func _ready() -> void:
	hotbar = _label(16)
	hotbar.set_anchors_and_offsets_preset(Control.PRESET_CENTER_BOTTOM)
	hotbar.grow_horizontal = Control.GROW_DIRECTION_BOTH
	hotbar.grow_vertical = Control.GROW_DIRECTION_BEGIN
	hotbar.position.y -= 16
	add_child(hotbar)

	panel = PanelContainer.new()
	panel.visible = false
	panel.set_anchors_and_offsets_preset(Control.PRESET_CENTER)
	panel.custom_minimum_size = Vector2(560, 420)
	panel.grow_horizontal = Control.GROW_DIRECTION_BOTH
	panel.grow_vertical = Control.GROW_DIRECTION_BOTH
	var box := VBoxContainer.new()
	panel.add_child(box)
	var title := _label(18)
	title.text = "Inventar   [I] schließen"
	box.add_child(title)
	list = ItemList.new()
	list.size_flags_vertical = Control.SIZE_EXPAND_FILL
	list.item_selected.connect(func(index: int) -> void: player.slot = list.get_item_metadata(index))
	box.add_child(list)
	details = _label(14)
	details.autowrap_mode = TextServer.AUTOWRAP_WORD
	box.add_child(details)
	add_child(panel)


func _label(size: int) -> Label:
	var label := Label.new()
	label.add_theme_font_size_override("font_size", size)
	var background := StyleBoxFlat.new()
	background.bg_color = Color(0.02, 0.03, 0.05, 0.72)
	background.set_content_margin_all(6)
	background.set_corner_radius_all(4)
	label.add_theme_stylebox_override("normal", background)
	return label


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and not event.echo:
		if event.physical_keycode == KEY_I:
			panel.visible = not panel.visible
			player.ui_open = panel.visible
			if panel.visible:
				Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
		elif event.physical_keycode >= KEY_1 and event.physical_keycode <= KEY_9:
			player.slot = event.physical_keycode - KEY_1
	elif event is InputEventMouseButton and event.pressed and not stacks.is_empty():
		if event.button_index == MOUSE_BUTTON_WHEEL_UP:
			_step(-1)
		elif event.button_index == MOUSE_BUTTON_WHEEL_DOWN:
			_step(1)


## Zum nächsten belegten Platz in Richtung `direction`.
func _step(direction: int) -> void:
	var slots: Array = stacks.map(func(stack: Dictionary) -> int: return stack.slot)
	var index := slots.find(player.slot)
	if index < 0:
		index = 0 if direction > 0 else slots.size() - 1
	else:
		index = posmod(index + direction, slots.size())
	player.slot = slots[index]


func _stack(slot: int) -> Dictionary:
	for stack: Dictionary in stacks:
		if stack.slot == slot:
			return stack
	return {}


func _process(delta: float) -> void:
	refresh += delta
	if refresh < 0.1:
		return
	refresh = 0.0
	stacks = terrain.get_inventory()
	var stack := _stack(player.slot)
	if not stack.is_empty():
		hotbar.text = "[B] bauen mit %s ×%d   Platz %d   [1-9, Mausrad] wählen   [I] Inventar" % [stack.name, stack.count, player.slot + 1]
	else:
		hotbar.text = "Inventar leer: graben füllt es   [I] Inventar" if stacks.is_empty() else "Platz %d leer   [1-9, Mausrad] wählen" % (player.slot + 1)
	if not panel.visible:
		return
	var lines: Array = stacks.map(func(s: Dictionary) -> String: return "%d: %s ×%d" % [s.slot + 1, s.name, s.count])
	var text := "\n".join(lines)
	if text != shown:
		shown = text
		list.clear()
		for i in stacks.size():
			list.add_item(lines[i])
			list.set_item_metadata(i, stacks[i].slot)
	list.deselect_all()
	for i in stacks.size():
		if stacks[i].slot == player.slot:
			list.select(i)
	details.text = "" if stack.is_empty() else "%s\n%s" % [stack.name, stack.details]
