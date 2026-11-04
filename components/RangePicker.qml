import QtQuick

CompactDropdown {
  id: picker

  required property var host

  showLabel: false
  fontFamily: picker.host.fontFamily
  options: [
    { value: "day", label: picker.host.t("costDay") },
    { value: "week", label: picker.host.t("costWeek") },
    { value: "month", label: picker.host.t("costMonth") }
  ]
  value: picker.host.costRange
  onChanged: function(picked) {
    picker.host.save("costRange", picked, "day")
    picker.value = Qt.binding(function() { return picker.host.costRange })
  }
}
